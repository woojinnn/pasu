//! Shared param helpers for the `facts/` namespace tree.
//!
//! The core helpers (`param_str`, `param_addr`, `param_u256`,
//! `param_token_contract`, `over_balance_4dp`, `OVER_BALANCE_SENTINEL`) are moved
//! VERBATIM from the Ground-pass `crates/simulation/server/src/facts.rs` — only
//! their visibility changes to `pub(super)` so every `<ns>` module can call them
//! through `super::params::*`, and `FactError` is imported from the shared
//! `super::FactError` (the router-owned enum) rather than redefined.
//!
//! The remaining helpers (`param_long`, `param_bool`, `param_decimal`,
//! `param_chain_id`, `param_asset_contract`, `param_action`) are pre-stocked from
//! the consolidated `paramHelpersNeeded` asks across the per-namespace
//! generators, so no owner needs to add a shared helper while filling in a body.
//!
//! Params arrive in the **lowered Cedar** shape resolved by the extension (see
//! the Ground-pass module header), NOT the `simulation-state` shape:
//!   - `chain_id` → a CAIP-2 string (`"eip155:1"`) for sim-server facts, or a
//!     numeric `Long` for the catalog methods typed `Long` — both forms are
//!     handled (`param_chain_id`).
//!   - `asset` / `token` / `value_asset` → lowered `Core::TokenRef`
//!     (`{ "key": { "standard", "chain", "address" } }`).
//!   - `amount` / `value_amount` / `gas_estimate` → hex `U256` strings.
//!   - `owner` / `spender` / `target` → plain hex address strings.
//!   - `action` → the lowered Cedar Action body (a JSON `Value`).

use serde_json::Value;

use policy_state::primitives::{Address, ChainId, U256};

use super::FactError;

// ---------------------------------------------------------------------------
// Moved VERBATIM from the Ground-pass facts.rs (visibility → pub(super) only).
// ---------------------------------------------------------------------------

/// Sentinel returned when `amountOverBalance` is unbounded (zero available
/// balance backing a positive approval). A deliberately huge 4-dp value so a
/// Cedar `.greaterThan(...)` threshold always trips.
pub(super) const OVER_BALANCE_SENTINEL: &str = "1000000000.0000";

/// Render `amount / divisor` to a 4-decimal-place string using U256 integer
/// math (no float — `amount` can be `U256::MAX`). Divisor zero → sentinel for a
/// positive numerator, `"0.0000"` for a zero numerator.
pub(super) fn over_balance_4dp(amount: U256, divisor: U256) -> String {
    if divisor.is_zero() {
        return if amount.is_zero() {
            "0.0000".to_owned()
        } else {
            OVER_BALANCE_SENTINEL.to_owned()
        };
    }
    // scaled = amount * 10_000 / divisor, then split into whole.frac4.
    let scale = U256::from(10_000u64);
    let scaled = amount.saturating_mul(scale) / divisor;
    let whole = scaled / scale;
    let frac = scaled % scale;
    format!("{whole}.{frac:04}")
}

pub(super) fn param_str(params: &Value, key: &str) -> Result<String, FactError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| FactError::BadParams(format!("missing string param `{key}`")))
}

pub(super) fn param_addr(params: &Value, key: &str) -> Result<Address, FactError> {
    let s = param_str(params, key)?;
    s.parse::<Address>()
        .map_err(|e| FactError::BadParams(format!("param `{key}` is not an address: {e}")))
}

pub(super) fn param_u256(params: &Value, key: &str) -> Result<U256, FactError> {
    let s = param_str(params, key)?;
    U256::from_str_radix(s.trim_start_matches("0x"), 16)
        .map_err(|e| FactError::BadParams(format!("param `{key}` is not a U256 hex: {e}")))
}

/// Extract the ERC20 contract address from the lowered `Core::TokenRef` param
/// shape (`{ "key": { "standard": "erc20", "address": "0x.." } }`). Only ERC20
/// is meaningful for an `approve` allowance; other standards are rejected.
pub(super) fn param_token_contract(params: &Value) -> Result<Address, FactError> {
    let key = params
        .get("token")
        .and_then(|t| t.get("key"))
        .ok_or_else(|| FactError::BadParams("missing param `token.key`".to_owned()))?;
    let standard = key.get("standard").and_then(Value::as_str);
    if standard != Some("erc20") {
        return Err(FactError::BadParams(format!(
            "token.key.standard is {standard:?}, expected \"erc20\""
        )));
    }
    let addr = key
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams("missing param `token.key.address`".to_owned()))?;
    addr.parse::<Address>()
        .map_err(|e| FactError::BadParams(format!("token.key.address is not an address: {e}")))
}

// ---------------------------------------------------------------------------
// Pre-stocked from the consolidated paramHelpersNeeded asks. Minimal correct
// implementations so no owner needs to add a shared helper during fill-in.
// ---------------------------------------------------------------------------

/// Read an integer (`Long`) param. Several catalog methods type `chain_id`,
/// `valid_until`, `deadline`, `domain_chain_id`, and `window_days` as JSON
/// `Long`s (not hex `U256` strings), which the Ground helpers did not cover.
pub(super) fn param_long(params: &Value, key: &str) -> Result<i64, FactError> {
    params
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| FactError::BadParams(format!("missing or non-integer Long param `{key}`")))
}

/// Read a boolean param (e.g. `allowances_cover_inputs` overrides).
#[allow(dead_code)]
pub(super) fn param_bool(params: &Value, key: &str) -> Result<bool, FactError> {
    params
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| FactError::BadParams(format!("missing or non-bool param `{key}`")))
}

/// Read a decimal-as-string param (e.g. an already-rendered `leverage` /
/// `expectedTokenPrice` carried by the lowered action). Returns the raw string;
/// the caller parses to whatever fixed-point representation it needs.
#[allow(dead_code)]
pub(super) fn param_decimal(params: &Value, key: &str) -> Result<String, FactError> {
    param_str(params, key)
}

/// Resolve `chain_id` to a [`ChainId`] from EITHER lowered form: the CAIP-2
/// string sim-server facts forward (`"eip155:1"`), or the numeric `Long` the
/// catalog types it as (`1` → `"eip155:1"`).
pub(super) fn param_chain_id(params: &Value, key: &str) -> Result<ChainId, FactError> {
    match params.get(key) {
        Some(Value::String(s)) => Ok(ChainId::new(s.clone())),
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => {
            Ok(ChainId::new(format!("eip155:{n}")))
        }
        _ => Err(FactError::BadParams(format!(
            "param `{key}` is not a chain id (CAIP-2 string or numeric Long)"
        ))),
    }
}

/// Generalized sibling of [`param_token_contract`]: extract the contract
/// [`Address`] of the lowered `Core::TokenRef`/`AssetRef` under an arbitrary key
/// (`asset`, `value_asset`, `token`, `claimToken`, …), accepting `erc20` /
/// `erc721` / `erc1155` standards. Native assets (no contract address) are
/// rejected — callers needing the native/gas token resolve it by chain instead.
pub(super) fn param_asset_contract(params: &Value, key: &str) -> Result<Address, FactError> {
    let inner = params
        .get(key)
        .and_then(|t| t.get("key"))
        .ok_or_else(|| FactError::BadParams(format!("missing param `{key}.key`")))?;
    let standard = inner.get("standard").and_then(Value::as_str);
    match standard {
        Some("erc20" | "erc721" | "erc1155") => {}
        other => {
            return Err(FactError::BadParams(format!(
                "{key}.key.standard is {other:?}, expected a contract standard"
            )))
        }
    }
    let addr = inner
        .get("address")
        .or_else(|| inner.get("contract"))
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams(format!("missing param `{key}.key.address`")))?;
    addr.parse::<Address>()
        .map_err(|e| FactError::BadParams(format!("{key}.key.address is not an address: {e}")))
}

/// Borrow the lowered Cedar Action body (the `action` param) as a JSON [`Value`]
/// so a fact can read `$.action.*` fields (size, side, leverage, newMode,
/// triggerPrice, orderKind, `reduce_only`, `live_inputs`.*, params.*, target,
/// calldata, …). The shape is action-kind-specific; callers reach into the
/// fields the catalog `stateDependency` declares for their method.
pub(super) fn param_action<'a>(params: &'a Value, key: &str) -> Result<&'a Value, FactError> {
    params
        .get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| FactError::BadParams(format!("missing Action param `{key}`")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn over_balance_4dp_math() {
        assert_eq!(
            over_balance_4dp(U256::from(5_000u64), U256::from(2_000u64)),
            "2.5000"
        );
        assert_eq!(
            over_balance_4dp(U256::from(1u64), U256::from(3u64)),
            "0.3333"
        );
        assert_eq!(over_balance_4dp(U256::ZERO, U256::ZERO), "0.0000");
        assert_eq!(
            over_balance_4dp(U256::from(1u64), U256::ZERO),
            OVER_BALANCE_SENTINEL
        );
    }

    #[test]
    fn param_chain_id_accepts_string_and_long() {
        assert_eq!(
            param_chain_id(&json!({ "chain_id": "eip155:1" }), "chain_id").unwrap(),
            ChainId::ethereum_mainnet()
        );
        assert_eq!(
            param_chain_id(&json!({ "chain_id": 1 }), "chain_id").unwrap(),
            ChainId::ethereum_mainnet()
        );
        assert!(param_chain_id(&json!({}), "chain_id").is_err());
    }

    #[test]
    fn param_long_reads_integers_only() {
        let ok = json!({ "valid_until": 42 });
        assert_eq!(param_long(&ok, "valid_until").unwrap(), 42);
        let bad = json!({ "valid_until": "42" });
        assert!(param_long(&bad, "valid_until").is_err());
    }

    #[test]
    fn param_asset_contract_accepts_nft_standards() {
        let v = json!({
            "asset": {
                "key": {
                    "standard": "erc721",
                    "contract": "0x00000000000000000000000000000000deadbeef"
                }
            }
        });
        assert!(param_asset_contract(&v, "asset").is_ok());
        let native = json!({ "asset": { "key": { "standard": "native" } } });
        assert!(param_asset_contract(&native, "asset").is_err());
    }

    #[test]
    fn param_action_requires_present_non_null() {
        let v = json!({ "action": { "size": "0x1" } });
        assert!(param_action(&v, "action").is_ok());
        assert!(param_action(&json!({ "action": null }), "action").is_err());
        assert!(param_action(&json!({}), "action").is_err());
    }
}
