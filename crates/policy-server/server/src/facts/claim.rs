//! `claim.*` enrichment-fact namespace — airdrop/claim safety facts (sim-server
//! fact host, ADR-009).
//!
//! Auto-generated stub scaffold: one arm per sim-server `claim.*` method in the
//! method-catalog `planned` set (`schema/method-catalog.json`, namespace
//! `claim.`). External-server `claim.*` methods, if any, are NOT scaffolded here
//! — they deploy to a different layer, not `facts.rs`.
//!
//! The inner [`dispatch`] match is FROZEN at scaffold time: it mirrors the
//! catalog registry exactly and must never be hand-edited, so the
//! `catalog_conformance` drift test keeps passing. Devs fill in the per-method
//! `fn` bodies (currently [`FactError::NotImplemented`]) and leave the match arms
//! untouched.
//!
//! ## Param shape contract
//!
//! Like the rest of `facts/`, `params` arrive as **lowered Cedar** shapes from
//! the extension (not `simulation-state` shapes) — see the sibling
//! `facts/params.rs` helpers:
//!   - `chain_id`: string (e.g. `"eip155:1"`)
//!   - `owner`: hex address string
//!   - `token`: lowered `Core::TokenRef`
//!     (`{ "key": { "standard", "chain", "address" } }`)
//!   - `action`: the lowered Claim action (plus, for the bundle fold, its
//!     enclosing multicall sibling sub-actions)

use serde_json::{json, Value};

use policy_state::primitives::{Address, ChainId, U256};
use policy_state::token::TokenKey;

use super::params::{param_action, param_str};
use super::FactCtx;
use super::FactError;

/// Reconstruct a full [`TokenKey`] from a lowered `Core::TokenRef` / `Core::TokenKey`
/// value (`{ "key": { "standard", "chain", "address" | "contract"+"tokenId" } }`,
/// or the bare `{ "standard", ... }` body).
///
/// Unlike [`super::params::param_asset_contract`] (which yields only an `Address`
/// and rejects native), held-set membership needs the WHOLE key — including the
/// `Native` chain key and the ERC721/1155 `token_id` — so the claimed token can
/// be looked up in `WalletState.tokens` by exact key.
fn token_key_from_lowered(token: &Value) -> Result<TokenKey, FactError> {
    let key = token.get("key").unwrap_or(token);
    let standard = key
        .get("standard")
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams("missing `token.key.standard`".to_owned()))?;
    let chain = key
        .get("chain")
        .and_then(Value::as_str)
        .map(|c| ChainId::new(c.to_owned()))
        .ok_or_else(|| FactError::BadParams("missing `token.key.chain`".to_owned()))?;

    let parse_addr = |field: &str| -> Result<Address, FactError> {
        key.get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| FactError::BadParams(format!("missing `token.key.{field}`")))?
            .parse::<Address>()
            .map_err(|e| {
                FactError::BadParams(format!("`token.key.{field}` is not an address: {e}"))
            })
    };
    let parse_token_id = || -> Result<U256, FactError> {
        let s = key
            .get("tokenId")
            .and_then(Value::as_str)
            .ok_or_else(|| FactError::BadParams("missing `token.key.tokenId`".to_owned()))?;
        U256::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|e| {
            FactError::BadParams(format!("`token.key.tokenId` is not a U256 hex: {e}"))
        })
    };

    match standard {
        "native" => Ok(TokenKey::Native { chain }),
        "erc20" => Ok(TokenKey::Erc20 {
            chain,
            address: parse_addr("address")?,
        }),
        "erc721" => Ok(TokenKey::Erc721 {
            chain,
            contract: parse_addr("contract")?,
            token_id: parse_token_id()?,
        }),
        "erc1155" => Ok(TokenKey::Erc1155 {
            chain,
            contract: parse_addr("contract")?,
            token_id: parse_token_id()?,
        }),
        other => Err(FactError::BadParams(format!(
            "`token.key.standard` is {other:?}, not a recognised standard"
        ))),
    }
}

/// Dispatch a `claim.*` enrichment method against `ctx`.
///
/// FROZEN: one arm per sim-server `claim.*` catalog method, plus a catch-all.
/// Do not edit — fill in the per-method `fn` bodies instead.
///
/// # Errors
///
/// [`FactError::UnknownMethod`] for an unregistered method; per-method errors
/// ([`FactError::NotImplemented`] / [`FactError::BadParams`]) propagate from the
/// individual fact fns.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "claim.source_held_state" => source_held_state(params, ctx),
        "claim.upfront_payment_required" => upfront_payment_required(params, ctx),
        "claim.token_unrecognized" => token_unrecognized(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// `claim.source_held_state` (readKind: direct) — AIR-02/03.
///
/// Whether the airdrop claim's source contract matches a known-eligible claim in
/// wallet state, and whether the claimed token is ALREADY held
/// (gasless-permit-on-held-token scam). Emits TWO context fields from one call.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `owner`: String (required) — `$.root.from`
/// - `action`: Action (required) — `$.action`; the claim action carrying the
///   source contract + claimed token
///
/// Catalog outputs:
/// - `sourceMatches`: Bool — from `$.result.sourceMatches`
/// - `tokenAlreadyHeld`: Bool — from `$.result.tokenAlreadyHeld`
///
/// State accessors the implementer should call:
/// - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — `tokenAlreadyHeld`
///   = the claimed token's `TokenKey` is a key in this map (held-set membership).
/// - `WalletState.positions: Vec<Position>` — locate the
///   `positions(airdrop_claim)` entry for (owner) and read its `source` to
///   compare against the action's source contract for `sourceMatches`.
// RESOLVED (no state-worker accessor needed): the airdrop-claim `source` is read
// by pattern-matching the public `PositionKind::AirdropClaim(a)` enum directly and
// comparing `a.source.name` — a typed getter is unnecessary.
fn source_held_state(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    // `owner` is required by the catalog; reading it asserts the param shape even
    // though held-set membership is keyed by the claimed token's own TokenKey.
    let _owner = param_str(params, "owner")?;
    let action = param_action(params, "action")?;

    // `tokenAlreadyHeld`: the claimed token's exact TokenKey is a key in
    // `WalletState.tokens` (held-set membership) — the gasless-permit-on-held
    // scam signal.
    let claim_token = action
        .get("claimToken")
        .ok_or_else(|| FactError::BadParams("missing `action.claimToken`".to_owned()))?;
    let token_key = token_key_from_lowered(claim_token)?;
    let token_already_held = ctx.state.tokens.contains_key(&token_key);

    // `sourceMatches`: does the action's distribution `source` (a ProtocolRef,
    // lowered to `{ name, .. }`) match a known airdrop-claim Position's source?
    // Compared on `ProtocolRef.name`, the stable distributor identifier; the
    // lowered action carries no other source discriminator.
    let source_name = action
        .get("source")
        .and_then(|s| s.get("name"))
        .and_then(Value::as_str);
    let source_matches = source_name.is_some_and(|name| {
        ctx.state.positions.iter().any(|p| {
            matches!(
                &p.kind,
                policy_state::position::PositionKind::AirdropClaim(a)
                    if a.source.name == name
            )
        })
    });

    Ok(json!({
        "sourceMatches": source_matches,
        "tokenAlreadyHeld": token_already_held,
    }))
}

/// `claim.upfront_payment_required` (readKind: fold) — AIR-06.
///
/// Does the claim bundle require an UPFRONT payment? True iff an ERC20 `approve`
/// OR a native (`msg.value > 0`) transfer is ordered BEFORE the claim sub-action
/// inside the same multicall bundle (advance-fee airdrop scam). A fold over the
/// bundle's preceding sub-actions; the single lowered Claim context cannot see
/// its siblings.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id` (CAIP-2 numeric chain id)
/// - `action`: Action (required) — `$.action`; the claim action plus its
///   enclosing bundle context, so the fold can inspect sub-actions ordered before
///   the claim
///
/// Catalog outputs:
/// - `requiresUpfrontPayment`: Bool — from `$.result.requiresUpfrontPayment`
///
/// State accessors the implementer should call:
/// - None. `stateDependency` is "action structure only (multicall sibling
///   sub-actions ordered before the claim); no wallet-state read" — this fold
///   reads the bundle/action shape, not `WalletState`.
fn upfront_payment_required(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    // BLOCKED: the fold needs the bundle's preceding sibling sub-actions
    // (an erc20 `approve` OR a native `msg.value > 0` transfer ordered BEFORE
    // the claim) but no such field exists on the lowered input.
    //   - `$.action` is the single `Airdrop::ClaimContext` — it carries NO
    //     siblings/bundle context (schema `actions/airdrop/claim.cedarschema`:
    //     ClaimContext has no preceding-actions field; the catalog itself notes
    //     "the single lowered Claim context cannot see its siblings").
    //   - The multicall lowering (`Core::MulticallContext`) only projects each
    //     child to `{ domain, action }` in an UNORDERED `Set` with no `amount`,
    //     no `msg.value`, and no claim-position marker — it cannot answer
    //     "approve/native-value ordered before the claim".
    // Missing field: an ordered preceding-sub-actions list on the lowered claim
    // (or a bundle context carrying each child's amount + native msg.value +
    // order index). Greenfield lowering work — cannot be synthesised from
    // existing fields/state.
    Err(FactError::NotImplemented(
        "claim.upfront_payment_required".into(),
    ))
}

/// `claim.token_unrecognized` (readKind: external) — AIR-07.
///
/// Is the claimed token unrecognised on BOTH counts? True iff the token is NOT a
/// key in `WalletState.tokens` (never held — durable state read) AND NOT present
/// in an external token list. The AND reduces the false positives of
/// held-history alone (normal airdrops are first-time tokens).
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id` (CAIP-2 numeric chain id)
/// - `owner`: String (required) — `$.root.from`; owner of the held-token set
///   checked for membership
/// - `token`: `AssetRef` (required) — `$.action.claimToken`; the token to be
///   received, normalised to a `TokenKey` for held-set membership and
///   external-list lookup
///
/// Catalog outputs:
/// - `tokenUnknown`: Bool — from `$.result.tokenUnknown`
///
/// State accessors the implementer should call:
/// - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — the held-key-set
///   half: `not held` = the claimed token's `TokenKey` is absent from this map.
///   (The external-token-list half is an external registry probe, not a state
///   read; AND the two for `tokenUnknown`.)
fn token_unrecognized(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    // `owner` is required by the catalog; membership is keyed by the token's
    // own TokenKey, so reading `owner` only asserts the param shape.
    let _owner = param_str(params, "owner")?;
    let token = params
        .get("token")
        .ok_or_else(|| FactError::BadParams("missing param `token`".to_owned()))?;
    let token_key = token_key_from_lowered(token)?;

    // Held-set half (durable state read): the claimed token has never been held
    // iff its exact TokenKey is absent from `WalletState.tokens`.
    let not_held = !ctx.state.tokens.contains_key(&token_key);

    // PARTIAL: the catalog defines `tokenUnknown = notHeld AND not-in-external-
    // token-list`, but `FactCtx` exposes only `state: &WalletState` — there is
    // no external token-list / registry handle here (`stateDependency` flags the
    // external list as "not the state DB"). The external probe can only ever
    // SHRINK the unknown set (a token on a known list is recognised), so
    // emitting `notHeld` is the conservative over-approximation: it never
    // under-warns. Wiring the external list in is an additive `FactCtx` field,
    // not a change to this fn's signature.
    Ok(json!({ "tokenUnknown": not_held }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::position::{AirdropClaim, ClaimStatus, Position, PositionKind};
    use policy_state::primitives::{ProtocolRef, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, TokenKind};
    use policy_state::token::TokenRef;
    use policy_state::{WalletId, WalletState};

    const CLAIM_TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const OWNER: &str = "0x000000000000000000000000000000000000a01c";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn claim_token_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: Address::from_str(CLAIM_TOKEN).unwrap(),
        }
    }

    fn wallet() -> WalletState {
        WalletState::new(WalletId::new(Address::from_str(OWNER).unwrap(), [chain()]))
    }

    fn oracle_source() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Pyth,
            feed_id: "x".into(),
        }
    }

    /// Insert a fungible holding of the claimed ERC20 (makes it "already held").
    fn with_claim_token_held(mut state: WalletState) -> WalletState {
        state.tokens.insert(
            claim_token_key(),
            TokenHolding {
                key: claim_token_key(),
                kind: TokenKind::Base {
                    category: BaseCategory::Stable,
                    peg_to: None,
                },
                symbol: "USDC".to_owned(),
                decimals: 6,
                balance: Balance::fungible(U256::from(1_000u64)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: None,
                metadata: None,
                value_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: oracle_source(),
            },
        );
        state
    }

    /// Insert an `AirdropClaim` Position whose `source.name` is `protocol`.
    fn with_airdrop_position(mut state: WalletState, protocol: &str) -> WalletState {
        state.positions.push(Position {
            id: "air-1".into(),
            protocol: ProtocolRef::new(protocol),
            chain: Some(chain()),
            kind: PositionKind::AirdropClaim(AirdropClaim {
                source: ProtocolRef::new(protocol),
                claimable: TokenRef {
                    key: claim_token_key(),
                },
                amount: U256::from(5_000_000u64),
                proof: None,
                claim_window: None,
                status: ClaimStatus::Claimable,
            }),
            primitives_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: oracle_source(),
        });
        state
    }

    /// Lowered `Core::TokenRef` for the claimed ERC20 (extension wire shape).
    fn claim_token_param() -> Value {
        json!({
            "key": {
                "standard": "erc20",
                "chain": chain().to_string(),
                "address": CLAIM_TOKEN
            }
        })
    }

    /// Lowered `Airdrop::ClaimContext` carrying `source.name` + `claimToken`.
    fn claim_action(source: &str) -> Value {
        json!({
            "source": { "name": source },
            "claimToken": claim_token_param()
        })
    }

    fn source_held_params(source: &str) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": OWNER,
            "action": claim_action(source)
        })
    }

    #[test]
    fn source_held_state_matches_known_airdrop_and_held_token() {
        let state = with_airdrop_position(with_claim_token_held(wallet()), "optimism");
        let out = dispatch(
            "claim.source_held_state",
            &source_held_params("optimism"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["sourceMatches"], json!(true));
        assert_eq!(out["tokenAlreadyHeld"], json!(true));
    }

    #[test]
    fn source_held_state_unknown_source_and_unheld_token() {
        // No matching airdrop Position, claimed token never held.
        let state = with_airdrop_position(wallet(), "optimism");
        let out = dispatch(
            "claim.source_held_state",
            &source_held_params("scam_protocol"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["sourceMatches"], json!(false));
        assert_eq!(out["tokenAlreadyHeld"], json!(false));
    }

    #[test]
    fn token_unrecognized_true_when_never_held() {
        let state = wallet();
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": OWNER,
            "token": claim_token_param()
        });
        let out = dispatch(
            "claim.token_unrecognized",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["tokenUnknown"], json!(true));
    }

    #[test]
    fn token_unrecognized_false_when_held() {
        let state = with_claim_token_held(wallet());
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": OWNER,
            "token": claim_token_param()
        });
        let out = dispatch(
            "claim.token_unrecognized",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["tokenUnknown"], json!(false));
    }

    #[test]
    fn token_key_from_lowered_handles_native_and_nft() {
        let native = json!({ "key": { "standard": "native", "chain": chain().to_string() } });
        assert_eq!(
            token_key_from_lowered(&native).unwrap(),
            TokenKey::Native { chain: chain() }
        );
        let nft = json!({
            "key": {
                "standard": "erc721",
                "chain": chain().to_string(),
                "contract": CLAIM_TOKEN,
                "tokenId": "0xff"
            }
        });
        assert_eq!(
            token_key_from_lowered(&nft).unwrap(),
            TokenKey::Erc721 {
                chain: chain(),
                contract: Address::from_str(CLAIM_TOKEN).unwrap(),
                token_id: U256::from(255u64),
            }
        );
    }

    #[test]
    fn upfront_payment_required_is_blocked() {
        let state = wallet();
        let err = dispatch(
            "claim.upfront_payment_required",
            &json!({ "chain_id": chain().to_string(), "action": claim_action("optimism") }),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
    }
}
