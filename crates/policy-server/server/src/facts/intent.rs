//! `intent.*` enrichment-fact stubs — signed off-chain intent / limit-order
//! facts served by the **sim-server** fact host (NOT the external policy-rpc
//! daemon). Generated from `schema/method-catalog.json` `planned` entries whose
//! `name` starts with `intent.` AND whose `server == "sim-server"`.
//!
//! The two `intent.*` methods with `server == "external"`
//! (`intent.fill_vs_floor`, `intent.isolated_fill_risk`) are intentionally
//! ABSENT here — they deploy to the external layer, not this fact host.
//!
//! Each fact below is a not-implemented stub: the dispatch arm exists (so the
//! method is routable and the server boots), but the body returns
//! [`FactError::NotImplemented`] until a dev fills it in. The inner `dispatch`
//! match is COMPLETE and FROZEN at scaffold time — do not edit it when wiring
//! bodies; add logic inside the per-method fns only.

use serde_json::{json, Value};

use policy_state::pending::{NonceKey, PendingKind};
use policy_state::primitives::{ChainId, U256};
use policy_state::token::TokenKey;

use super::params::{param_action, param_chain_id, param_str, param_u256};
use super::FactCtx;
use super::FactError;

/// Route an `intent.*` method to its fact implementation.
///
/// FROZEN: one arm per sim-server method in this namespace plus the catch-all.
/// Devs filling in bodies must never edit this match.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "intent.validity_horizon_sec" => validity_horizon_sec(params, ctx),
        "intent.pending_cap_over_balance" => pending_cap_over_balance(params, ctx),
        "intent.near_duplicate_pending" => near_duplicate_pending(params, ctx),
        "intent.cancel_target_missing" => cancel_target_missing(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// Resolve a lowered `Core::TokenRef` value (`{ key: { standard, chain,
/// address } }`) into a [`TokenKey`] for matching against pending-order
/// commitments / sell legs. Only the `native` and `erc20` standards carry a
/// fungible sell leg; NFT standards are not valid intent-order sell legs and
/// return `None`.
fn lowered_token_key(token: &Value) -> Option<TokenKey> {
    let key = token.get("key")?;
    let standard = key.get("standard").and_then(Value::as_str)?;
    let chain = ChainId::new(key.get("chain").and_then(Value::as_str)?.to_owned());
    match standard {
        "native" => Some(TokenKey::Native { chain }),
        "erc20" => {
            let address = key.get("address").and_then(Value::as_str)?.parse().ok()?;
            Some(TokenKey::Erc20 { chain, address })
        }
        _ => None,
    }
}

/// Read the lowered `sell` token leg from a `sign_intent_order` action body.
fn action_sell_key(action: &Value) -> Result<TokenKey, FactError> {
    action
        .get("sell")
        .and_then(lowered_token_key)
        .ok_or_else(|| FactError::BadParams("action.sell is not a fungible TokenRef".to_owned()))
}

/// UNI-11 `intent.validity_horizon_sec` — seconds from the evaluation clock
/// (now) to a signed intent order's expiry (`validUntil - now`). A long horizon
/// makes a signed open order a free option for the filler/solver.
///
/// readKind: `derived`
///
/// Catalog params:
/// - `valid_until: Long` (required) — `$.action.validUntil`, signed order expiry
///   (unix seconds, `Amm::SignIntentOrderContext.validUntil`).
///
/// Catalog outputs:
/// - `horizonSec: Long` — `$.result.horizonSec`.
///
/// State accessors: NONE on `WalletState`. The horizon needs the **evaluation
/// clock (sim-server now)**, which is not part of the wallet snapshot.
// BLOCKED: needs the evaluation clock `now` (server wall-clock / `EvalContext.now`).
// `FactCtx` exposes ONLY `state: &WalletState`; the wallet snapshot carries no
// wall-clock, and `horizonSec = validUntil - now` cannot be computed without it.
// Unblocks when `FactCtx` gains a `now: Time` field (additive per the FROZEN
// signature carrier note in facts/mod.rs).
fn validity_horizon_sec(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let _ = (params, ctx);
    Err(FactError::NotImplemented(
        "intent.validity_horizon_sec".into(),
    ))
}

/// AMMLP-3 `intent.pending_cap_over_balance` — does the sum of active off-chain
/// intent-order `PermitCap`/`SpendCap`s on the sell token, PLUS the new order's
/// sell amount, exceed the wallet's held balance of that token? A reducer State₂
/// fold over the `OffchainLimitOrder` pending set against token holdings.
///
/// readKind: `reducer`
///
/// Catalog params:
/// - `chain_id: Long` (required) — `$.root.chain_id`.
/// - `owner: String` (required) — `$.root.from`, wallet whose pending caps and
///   balance are folded.
/// - `action: Action` (required) — `$.action`, the new `sign_intent_order`
///   whose sell amount is added to the active-cap sum.
///
/// Catalog outputs:
/// - `capSumOverBalance: Bool` — `$.result.capSumOverBalance`.
///
/// State accessors:
/// - `WalletState.pending: Vec<PendingTx>` — fold the active
///   (`is_active_or_partial`) entries' `AssetCommitment::cap_for(sell_key)`.
/// - `WalletState::available_balance(&self, key: &TokenKey) -> Option<U256>` —
///   the held balance the cap sum is compared against.
fn pending_cap_over_balance(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let _chain = param_chain_id(params, "chain_id")?;
    let _owner = param_str(params, "owner")?;
    let action = param_action(params, "action")?;

    let sell_key = action_sell_key(action)?;
    let new_sell = param_u256(action, "sellAmount")?;

    // Fold the committed caps of active intent orders on the same sell token.
    // `cap_for` already filters by token key and contributes only for
    // SpendCap/PermitCap (HardLock is already reflected in the balance).
    let active_cap_sum = ctx
        .state
        .pending
        .iter()
        .filter(|p| p.lifecycle.is_active_or_partial())
        .filter(|p| matches!(p.kind, PendingKind::OffchainLimitOrder { .. }))
        .fold(U256::ZERO, |acc, p| {
            acc.saturating_add(p.commitment.cap_for(&sell_key))
        });

    let cap_sum = active_cap_sum.saturating_add(new_sell);
    let balance = ctx.state.available_balance(&sell_key).unwrap_or(U256::ZERO);

    Ok(json!({ "capSumOverBalance": cap_sum > balance }))
}

/// AMMLP-4 `intent.near_duplicate_pending` — is the new intent order a
/// near-duplicate of an already-open one (same venue, sell, buy) in the active
/// `OffchainLimitOrder` pending set? A reducer State₂ membership check.
/// true = likely accidental re-sign (double size / malicious double-fill).
///
/// readKind: `reducer`
///
/// Catalog params:
/// - `chain_id: Long` (required) — `$.root.chain_id`.
/// - `owner: String` (required) — `$.root.from`, wallet whose pending-order set
///   is searched for a near-duplicate.
/// - `action: Action` (required) — `$.action`, the new `sign_intent_order`
///   matched against the active order set.
///
/// Catalog outputs:
/// - `duplicate: Bool` — `$.result.duplicate`.
///
/// State accessors:
/// - `WalletState.pending: Vec<PendingTx>` — search the active
///   `OffchainLimitOrder` entries for a same-venue/sell/buy match.
///
/// Match semantics: same venue name, same sell `TokenKey`, same buy `TokenKey`.
/// The "similar cap" leg of the catalog description is approximated by venue +
/// sell + buy identity — a same-pair, same-venue resting order is the
/// duplicate-re-sign signal — because the lowered `competingOrders` / live caps
/// are not a reliable equality key. PARTIAL: cap-tolerance band not applied.
fn near_duplicate_pending(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let _chain = param_chain_id(params, "chain_id")?;
    let _owner = param_str(params, "owner")?;
    let action = param_action(params, "action")?;

    let new_sell = action_sell_key(action)?;
    let new_buy = action
        .get("buy")
        .and_then(lowered_token_key)
        .ok_or_else(|| FactError::BadParams("action.buy is not a fungible TokenRef".to_owned()))?;
    let new_venue = action
        .get("venue")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams("missing action.venue.name".to_owned()))?;

    let duplicate = ctx
        .state
        .pending
        .iter()
        .filter(|p| p.lifecycle.is_active_or_partial())
        .any(|p| match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue, sell, buy, ..
            } => venue.name == new_venue && sell.key == new_sell && buy.key == new_buy,
            _ => false,
        });

    Ok(json!({ "duplicate": duplicate }))
}

/// AMMLP-5 `intent.cancel_target_missing` — is the cancel's `orderHash` absent
/// from the wallet's active (`Active`/`PartiallyFilled`) intent-order set? A
/// reducer State₂ membership check. true = cancelling an already-filled /
/// expired / cancelled order (wasted signature or a phishing "looks-like-cancel"
/// prompt).
///
/// readKind: `reducer`
///
/// Catalog params:
/// - `chain_id: Long` (required) — `$.root.chain_id`.
/// - `owner: String` (required) — `$.root.from`, wallet whose active order set
///   is checked for the cancel target.
/// - `order_hash: String` (required) — `$.action.orderHash`, 32-byte hex order
///   hash being cancelled (`Amm::CancelIntentOrderContext.orderHash`).
///
/// Catalog outputs:
/// - `targetMissing: Bool` — `$.result.targetMissing`.
///
/// State accessors:
/// - `WalletState.pending: Vec<PendingTx>` — test `order_hash` for membership in
///   the active `OffchainLimitOrder` set, via either the pending `id`
///   (`PendingId` is the order hash/id string) or a
///   `lifecycle.nonce == NonceKey::OrderHash { hash }`.
fn cancel_target_missing(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let _chain = param_chain_id(params, "chain_id")?;
    let _owner = param_str(params, "owner")?;
    let order_hash = param_str(params, "order_hash")?;
    let target = order_hash.to_ascii_lowercase();

    let found = ctx
        .state
        .pending
        .iter()
        .filter(|p| p.lifecycle.is_active_or_partial())
        .filter(|p| matches!(p.kind, PendingKind::OffchainLimitOrder { .. }))
        .any(|p| {
            p.id.to_ascii_lowercase() == target
                || matches!(
                    &p.lifecycle.nonce,
                    Some(NonceKey::OrderHash { hash }) if hash.to_ascii_lowercase() == target
                )
        });

    Ok(json!({ "targetMissing": !found }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::delta::StateDelta;
    use policy_state::live_field::DataSource;
    use policy_state::pending::{
        AssetCommitment, NonceKey, OrderKind, PendingKind, PendingLifecycle, PendingStatus,
        PendingTx,
    };
    use policy_state::primitives::{Address, Time, VenueRef};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, TokenKind};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::{WalletId, WalletState};

    use serde_json::json;

    const SELL: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const BUY: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn addr(s: &str) -> Address {
        Address::from_str(s).unwrap()
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            addr("0x000000000000000000000000000000000000a01c"),
            [chain()],
        )
    }

    fn sell_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: addr(SELL),
        }
    }

    fn buy_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: addr(BUY),
        }
    }

    fn source() -> DataSource {
        DataSource::UserSupplied
    }

    /// A `WalletState` holding `balance` of the sell token.
    fn state_with_balance(balance: u64) -> WalletState {
        let mut state = WalletState::new(wallet_id());
        state.tokens.insert(
            sell_key(),
            TokenHolding {
                key: sell_key(),
                kind: TokenKind::Base {
                    category: BaseCategory::Stable,
                    peg_to: None,
                },
                symbol: "USDC".to_owned(),
                decimals: 6,
                balance: Balance::fungible(U256::from(balance)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: None,
                metadata: None,
                value_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: source(),
            },
        );
        state
    }

    /// An active `OffchainLimitOrder` pending tx selling `sell_key` with a
    /// `PermitCap` of `cap`, identified by `id`.
    fn offchain_order(id: &str, cap: u64, status: PendingStatus) -> PendingTx {
        PendingTx {
            id: id.to_owned(),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef::new("uniswap_x"),
                sell: TokenRef { key: sell_key() },
                buy: TokenRef { key: buy_key() },
                sell_max: U256::from(cap),
                buy_min: U256::ZERO,
                order_kind: OrderKind::Dutch,
            },
            commitment: AssetCommitment::PermitCap {
                token: TokenRef { key: sell_key() },
                spender: addr("0x00000000000000000000000000000000deadbeef"),
                max_out: U256::from(cap),
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status,
                valid_until: None,
                nonce: Some(NonceKey::OrderHash {
                    hash: id.to_owned(),
                }),
                on_chain_tx: None,
                raw_status: None,
            },
            sync: source(),
            signed_at: Time::from_unix(1_700_000_000),
            signature_payload: Vec::new(),
        }
    }

    /// Lowered `sign_intent_order` action body (the shape lowering emits).
    fn sign_action(sell_amount_hex: &str) -> Value {
        json!({
            "venue": { "name": "uniswap_x", "chain": chain().to_string() },
            "sell": { "key": { "standard": "erc20", "chain": chain().to_string(), "address": SELL } },
            "buy": { "key": { "standard": "erc20", "chain": chain().to_string(), "address": BUY } },
            "sellAmount": sell_amount_hex,
            "buyMin": "0x0",
            "orderKind": "dutch",
            "recipient": "0x000000000000000000000000000000000000a01c",
            "validUntil": 1_738_003_600u64
        })
    }

    fn fold_params(action: &Value) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "action": action
        })
    }

    #[test]
    fn cap_over_balance_trips_when_active_caps_plus_new_exceed_balance() {
        // 600 active cap + 600 new sell = 1200 > 1000 balance → true.
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 600, PendingStatus::Active));
        let new_sell = format!("{:#x}", U256::from(600u64));
        let out = dispatch(
            "intent.pending_cap_over_balance",
            &fold_params(&sign_action(&new_sell)),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["capSumOverBalance"], json!(true));
    }

    #[test]
    fn cap_over_balance_false_within_balance() {
        // 100 active cap + 100 new sell = 200 <= 1000 balance → false.
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 100, PendingStatus::Active));
        let new_sell = format!("{:#x}", U256::from(100u64));
        let out = dispatch(
            "intent.pending_cap_over_balance",
            &fold_params(&sign_action(&new_sell)),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["capSumOverBalance"], json!(false));
    }

    #[test]
    fn cap_over_balance_ignores_inactive_orders() {
        // A cancelled 5000-cap order must NOT count toward the active sum.
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 5_000, PendingStatus::Cancelled));
        let new_sell = format!("{:#x}", U256::from(100u64));
        let out = dispatch(
            "intent.pending_cap_over_balance",
            &fold_params(&sign_action(&new_sell)),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["capSumOverBalance"], json!(false));
    }

    #[test]
    fn near_duplicate_detects_same_venue_pair() {
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 100, PendingStatus::Active));
        let out = dispatch(
            "intent.near_duplicate_pending",
            &fold_params(&sign_action("0x64")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["duplicate"], json!(true));
    }

    #[test]
    fn near_duplicate_false_when_no_active_match() {
        // Only an inactive (filled) order on the same pair → not a duplicate.
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 100, PendingStatus::Filled));
        let out = dispatch(
            "intent.near_duplicate_pending",
            &fold_params(&sign_action("0x64")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["duplicate"], json!(false));
    }

    fn cancel_params(order_hash: &str) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "order_hash": order_hash
        })
    }

    #[test]
    fn cancel_target_present_by_id_is_not_missing() {
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xAA", 100, PendingStatus::Active));
        // Case-insensitive id match.
        let out = dispatch(
            "intent.cancel_target_missing",
            &cancel_params("0xaa"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["targetMissing"], json!(false));
    }

    #[test]
    fn cancel_target_absent_is_missing() {
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 100, PendingStatus::Active));
        let out = dispatch(
            "intent.cancel_target_missing",
            &cancel_params("0xbb"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["targetMissing"], json!(true));
    }

    #[test]
    fn cancel_target_inactive_order_is_missing() {
        // An expired order with the hash is not an active cancel target.
        let mut state = state_with_balance(1_000);
        state
            .pending
            .push(offchain_order("0xaa", 100, PendingStatus::Expired));
        let out = dispatch(
            "intent.cancel_target_missing",
            &cancel_params("0xaa"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["targetMissing"], json!(true));
    }

    #[test]
    fn validity_horizon_is_blocked_pending_eval_clock() {
        let state = WalletState::new(wallet_id());
        let err = dispatch(
            "intent.validity_horizon_sec",
            &json!({ "valid_until": 1_738_003_600u64 }),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
    }
}
