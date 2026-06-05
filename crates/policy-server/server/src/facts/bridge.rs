//! `bridge.*` enrichment-fact namespace — sim-server fact host.
//!
//! Auto-generated stub scaffold (one arm per sim-server `bridge.*` method in the
//! method-catalog `planned` set). The inner [`dispatch`] match is FROZEN at
//! scaffold time: it mirrors the catalog registry exactly and must never be
//! hand-edited. Devs fill in the per-method `fn` bodies (currently
//! [`FactError::NotImplemented`]) but leave the match arms untouched so the
//! `catalog_conformance` drift test keeps passing.
//!
//! Param shapes arrive as **lowered Cedar** values (not `simulation-state`
//! shapes), resolved by the extension before the call — see the sibling
//! `facts/params.rs` helpers (`chain_id` string, lowered `AssetRef`/`TokenRef`,
//! hex `U256` amounts). Bridge facts target `Core::Unknown` actions, so they
//! carry the raw `target` + `calldata` (0x-hex) the fact must introspect.
//!
//! The `external` `bridge.*` methods (`permission_change`,
//! `cctp_recipient_unreceivable`) are NOT here: they deploy to the external/local
//! layer, not this sim-server fact host.
//!
//! ## Fill-in status (state-worker)
//!
//! Three of the four methods (`dest_chain_supported`, `relayer_fee_bp`,
//! `min_out_haircut_bp`) need to pull a *typed argument at a target-specific ABI
//! offset* out of an `Unknown` bridge `calldata` (the destination-chain EID/CCTP
//! domain, the Across `inputAmount`/`outputAmount`, the liquidity-bridge
//! `minAmountLD`). That requires a per-`target` calldata-layout / selector table
//! to know *which 32-byte word* holds the value — and the server crate has NO ABI
//! decoder dependency (`alloy-dyn-abi` / `alloy-sol-types` are not deps; only
//! `simulation-state` + `serde_json`), and no such layout registry exists in the
//! readable state surface. Guessing an offset would feed a *fabricated number*
//! into a numeric Cedar threshold, so those three stay BLOCKED with the exact
//! missing input named inline. `refund_self` returns a Bool and the `owner`
//! address is a param, so it admits a layout-agnostic, warn-conservative scan
//! (see its doc) and is implemented as a documented approximation.

use serde_json::{json, Value};

use super::params::{param_addr, param_str};
use super::FactCtx;
use super::FactError;

/// Dispatch a `bridge.*` enrichment method against `ctx`.
///
/// FROZEN: one arm per sim-server `bridge.*` catalog method, plus a catch-all.
/// Do not edit — fill in the per-method `fn` bodies instead.
///
/// # Errors
///
/// [`FactError::UnknownMethod`] for an unregistered method; per-method errors
/// ([`FactError::NotImplemented`] / [`FactError::BadParams`]) propagate from the
/// individual fact fns.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "bridge.dest_chain_supported" => dest_chain_supported(params, ctx),
        "bridge.min_out_haircut_bp" => min_out_haircut_bp(params, ctx),
        "bridge.relayer_fee_bp" => relayer_fee_bp(params, ctx),
        "bridge.refund_self" => refund_self(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// Split a 0x-hex bridge `calldata` (after the 4-byte selector) into its 32-byte
/// ABI words, returning each word as its 64-hex-char (lowercase, no `0x`) string.
///
/// Layout-agnostic: it does not interpret any word, it just slices the head into
/// 32-byte boundaries so a caller can scan for a known value (e.g. an address in
/// the low 20 bytes of a word). A `calldata` shorter than the 4-byte selector
/// yields an empty iterator. Words are NOT validated as hex here; the caller
/// pattern-matches on the produced substrings.
fn calldata_words(calldata: &str) -> Vec<String> {
    let hex = calldata.strip_prefix("0x").unwrap_or(calldata);
    // ABI: 4-byte (8 hex char) selector, then 32-byte (64 hex char) words.
    let body = hex.get(8..).unwrap_or("");
    body.as_bytes()
        .chunks(64)
        .filter(|c| c.len() == 64)
        .map(|c| String::from_utf8_lossy(c).to_ascii_lowercase())
        .collect()
}

/// `bridge.dest_chain_supported` (readKind: derived) — BRG-04.
///
/// Is the destination chain of a bridge call (the `dstChainId` — `LayerZero` EID /
/// CCTP domain / etc. — encoded inside the Unknown `calldata`) a member of the
/// wallet's tracked chain set (`WalletId.chains`)? Extracts `dstChainId` from the
/// calldata then tests set membership. Emits a single Bool so the warn fires when
/// the destination chain is NOT one the wallet uses.
///
/// Params:
/// - `chain_id`: Long (required) — source EIP-155 chain ID.
/// - `target`: String (required) — bridge contract address, picks the calldata layout.
/// - `calldata`: String (required) — raw 0x-hex calldata carrying the destination chain id.
///
/// Outputs:
/// - `supported`: Bool — from `$.result.supported`
///
/// Accessors:
/// - `WalletState.wallet_id: WalletId` — the identity carrying `chains`
///   (`BTreeSet<ChainId>`); test the decoded `dstChainId` for membership in it.
fn dest_chain_supported(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    // BLOCKED: extracting `dstChainId` needs (a) a per-`target` calldata-layout
    // table to know which 32-byte word holds it, and (b) an EID/CCTP-domain ->
    // ChainId mapping to compare against `wallet_id.chains` — bridge dest-chain
    // values are LayerZero EIDs / CCTP domains, NOT EIP-155 chain ids, so a raw
    // word match is coincidental. Neither the target->layout table nor the
    // EID/domain->ChainId map exists in the state map or the server crate (no
    // ABI-decoder dep). Membership test against `ctx.state.wallet_id.chains` is
    // trivial once a real ChainId is decoded.
    Err(FactError::NotImplemented(
        "bridge.dest_chain_supported".into(),
    ))
}

/// `bridge.min_out_haircut_bp` (readKind: derived) — BRG-05.
///
/// How harsh is the minimum-received floor (`minAmountLD` / `amountOutMin`) baked
/// into a liquidity-bridge `calldata`, relative to a fair quote? Extracts the
/// min-out from the Unknown calldata, prices the send against the refrigerated
/// token price (`token_holdings.price_value`), and returns the haircut
/// `(fair_quote - min_out) / fair_quote` in basis points (Long).
///
/// Params:
/// - `chain_id`: Long (required) — source EIP-155 chain ID.
/// - `target`: String (required) — bridge contract address, picks the calldata layout.
/// - `calldata`: String (required) — raw 0x-hex calldata carrying the send amount and min-received floor.
///
/// Outputs:
/// - `haircutBp`: Long — from `$.result.haircutBp`
///
/// Accessors:
/// - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — look up the sent token's holding.
/// - `TokenHolding.price_usd: Option<LiveField<Price>>` — the refrigerated price for the fair quote.
fn min_out_haircut_bp(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    // BLOCKED: needs THREE inputs none of which are available:
    //   1. a per-`target` calldata-layout table giving the word offsets of the
    //      send `amount` and the min-out (`minAmountLD`/`amountOutMin`) — there is
    //      no ABI decoder dep and no layout registry;
    //   2. the identity of the SENT token — there is no `token`/`asset` param on
    //      this method (only `target` + `calldata`), so the `TokenHolding` to
    //      price cannot be selected without first decoding the token address from
    //      the (unknown-layout) calldata;
    //   3. consequently `TokenHolding.price_usd` (the fair-quote price) cannot be
    //      looked up. A wrong-offset guess would feed a fabricated `haircutBp`
    //      Long into a numeric Cedar threshold, so this is not approximable
    //      conservatively.
    Err(FactError::NotImplemented(
        "bridge.min_out_haircut_bp".into(),
    ))
}

/// `bridge.relayer_fee_bp` (readKind: derived) — BRG-09.
///
/// On an intent bridge (Across) the received amount is governed by the relayer
/// fee = input - output spread, not slippage. Decodes `inputAmount`/`outputAmount`
/// from the Unknown deposit `calldata` and returns the relayer fee in basis points
/// (Long) so a warn fires when the fee exceeds the usual band. Also surfaces the
/// variant where the UI-shown fee and the calldata `outputAmount` disagree.
///
/// Params:
/// - `chain_id`: Long (required) — source EIP-155 chain ID.
/// - `target`: String (required) — intent-bridge contract address, picks the deposit calldata layout.
/// - `calldata`: String (required) — raw 0x-hex deposit calldata carrying inputAmount and outputAmount.
///
/// Outputs:
/// - `relayerFeeBp`: Long — from `$.result.relayerFeeBp`
///
/// Accessors:
/// - (none) — derived purely from the Unknown deposit calldata
///   (`inputAmount`/`outputAmount`) vs a learned relayer-fee band; no wallet-state read.
fn relayer_fee_bp(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    // BLOCKED: computing `(inputAmount - outputAmount) / inputAmount` in bp needs
    // the word offsets of `inputAmount` and `outputAmount` in the Across deposit
    // calldata. That is a per-`target`/per-selector layout the server cannot
    // decode — no ABI-decoder dep (`alloy-dyn-abi`/`alloy-sol-types` absent) and
    // no deposit-calldata layout table in the state map or any provided input.
    // The bp arithmetic (U256 integer math, no Decimal ops) is trivial once both
    // amounts are decoded; a guessed offset would emit a fabricated bp Long.
    Err(FactError::NotImplemented("bridge.relayer_fee_bp".into()))
}

/// `bridge.refund_self` (readKind: derived) — BRG-12.
///
/// Is the bridge refund/recovery address (a separate calldata argument, distinct
/// from the recipient) NOT the wallet's own address? Introspects the Unknown
/// `calldata` for the refund/recovery address and compares it to the wallet owner,
/// returning a single Bool that is true when the refund address is not the wallet
/// (so a failed transfer would be unrecoverable).
///
/// Params:
/// - `chain_id`: Long (required) — chain ID of the bridge transaction.
/// - `owner`: String (required) — wallet owner address, the expected refund destination.
/// - `target`: String (required) — bridge contract address, picks the calldata layout.
/// - `calldata`: String (required) — raw 0x-hex calldata carrying the refund/recovery address argument.
///
/// Outputs:
/// - `refundNotSelf`: Bool — from `$.result.refundNotSelf`
///
/// Accessors:
/// - `WalletState.wallet_id: WalletId` — the identity carrying `address`; compare
///   the decoded refund/recovery address against it (the `owner` param mirrors this).
///
/// APPROXIMATION (partial): without a per-`target` calldata-layout table we
/// cannot point at the exact refund-arg word, so instead of guessing an offset we
/// run a layout-agnostic scan: split the calldata into 32-byte words and check
/// whether the `owner` address appears in the low 20 bytes of ANY word. If the
/// wallet's own address is referenced somewhere in the calldata, a self-refund is
/// plausible (`refundNotSelf = false`); if it is referenced nowhere, the refund
/// almost certainly routes elsewhere (`refundNotSelf = true`). This is the
/// warn-safe direction: the policy fires a *warn* (not a fail), and a false
/// "not self" is conservative. It can false-negative if a bridge embeds `owner`
/// in calldata for an unrelated field (recipient == refund), but that collapses
/// recipient/refund — an acceptable miss for a warn. `owner` is preferred over
/// `wallet_id.address` because the catalog passes it explicitly (`$.root.from`),
/// but they mirror each other.
fn refund_self(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let owner = param_addr(params, "owner")?;
    let calldata = param_str(params, "calldata")?;

    let owner_hex = format!("{owner:x}"); // 40 lowercase hex chars, no 0x.
    let words = calldata_words(&calldata);
    let owner_referenced = words
        .iter()
        .any(|word| owner_appears_in_word(word, &owner_hex));

    Ok(json!({ "refundNotSelf": !owner_referenced }))
}

/// True if a left-padded ABI address word holds `owner_hex` (40 lowercase hex
/// chars, no `0x`): the high 12 bytes (24 hex) must be zero and the low 20 bytes
/// must equal `owner_hex`. Rejecting non-zero padding avoids matching `owner` as
/// an incidental substring inside a `uint256` amount word.
fn owner_appears_in_word(word: &str, owner_hex: &str) -> bool {
    word.len() == 64
        && word[..24].bytes().all(|b| b == b'0')
        && word[24..].eq_ignore_ascii_case(owner_hex)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use policy_state::primitives::Address;

    use std::str::FromStr;

    use policy_state::primitives::ChainId;
    use policy_state::{WalletId, WalletState};

    const OWNER: &str = "0x000000000000000000000000000000000000a01c";
    const OTHER: &str = "0x0000000000000000000000000000000000000bad";

    fn state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from_str(OWNER).unwrap(),
            [ChainId::ethereum_mainnet()],
        ))
    }

    /// 4-byte selector + the given 32-byte words (each `right`-padded address or
    /// raw 64-hex word) concatenated into a 0x-hex calldata string.
    fn calldata(words: &[&str]) -> String {
        let mut s = String::from("0xdeadbeef");
        for w in words {
            s.push_str(w);
        }
        s
    }

    /// Left-pad a 40-hex address (no 0x) into a 32-byte ABI word.
    fn addr_word(addr_hex_no_0x: &str) -> String {
        format!("{addr_hex_no_0x:0>64}")
    }

    #[test]
    fn refund_to_self_is_not_flagged() {
        let owner_word = addr_word(&OWNER[2..]);
        let params = json!({
            "chain_id": "eip155:1",
            "owner": OWNER,
            "target": "0x0000000000000000000000000000000000bbbbbb",
            "calldata": calldata(&[&owner_word]),
        });
        let out = dispatch("bridge.refund_self", &params, &FactCtx { state: &state() }).unwrap();
        assert_eq!(out["refundNotSelf"], json!(false));
    }

    #[test]
    fn refund_elsewhere_is_flagged() {
        let other_word = addr_word(&OTHER[2..]);
        let params = json!({
            "chain_id": "eip155:1",
            "owner": OWNER,
            "target": "0x0000000000000000000000000000000000bbbbbb",
            "calldata": calldata(&[&other_word]),
        });
        let out = dispatch("bridge.refund_self", &params, &FactCtx { state: &state() }).unwrap();
        assert_eq!(out["refundNotSelf"], json!(true));
    }

    #[test]
    fn owner_inside_amount_word_does_not_count() {
        // The owner's 40 hex chars embedded in a non-zero-padded word (e.g. a
        // large amount) must NOT register as a refund-to-self address.
        let amount_word = format!("ffffffffffffffffffffffff{}", &OWNER[2..]);
        let params = json!({
            "chain_id": "eip155:1",
            "owner": OWNER,
            "target": "0x0000000000000000000000000000000000bbbbbb",
            "calldata": calldata(&[&amount_word]),
        });
        let out = dispatch("bridge.refund_self", &params, &FactCtx { state: &state() }).unwrap();
        assert_eq!(out["refundNotSelf"], json!(true));
    }

    #[test]
    fn missing_owner_is_bad_params() {
        let params = json!({
            "chain_id": "eip155:1",
            "target": "0x0000000000000000000000000000000000bbbbbb",
            "calldata": "0xdeadbeef",
        });
        let err =
            dispatch("bridge.refund_self", &params, &FactCtx { state: &state() }).unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn blocked_methods_stay_not_implemented() {
        for m in [
            "bridge.dest_chain_supported",
            "bridge.relayer_fee_bp",
            "bridge.min_out_haircut_bp",
        ] {
            let err = dispatch(m, &json!({}), &FactCtx { state: &state() }).unwrap_err();
            assert!(matches!(err, FactError::NotImplemented(_)), "{m}: {err:?}");
        }
    }

    #[test]
    fn words_skip_selector_and_chunk_by_32_bytes() {
        let w0 = "1".repeat(64);
        let w1 = "2".repeat(64);
        let cd = format!("0xaabbccdd{w0}{w1}");
        let words = calldata_words(&cd);
        assert_eq!(words, vec![w0, w1]);
    }
}
