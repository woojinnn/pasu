//! `reserve.*` enrichment-fact namespace — Aave reserve / isolation-mode facts.
//!
//! Scaffold module: the inner [`dispatch`] match is FROZEN at scaffold time (one
//! arm per sim-server `reserve.*` method in `schema/method-catalog.json`, plus a
//! catch-all). Devs fill in the per-method `fn` bodies; they must never edit the
//! match.
//!
//! These facts surface reserve-config / protocol-global metadata that the shipped
//! lending `ReserveState` context does NOT carry as base fields (siloed flag,
//! debt ceiling, isolationModeTotalDebt, numeric caps). `reserve.cap_used_bp` is
//! servable from the action's own `live_inputs.reserveState` (no wallet read);
//! the other two need reserve/protocol-global metadata not yet surfaced as a
//! Ground accessor — flagged below as STATE-WORKER ASK items.

use serde_json::{json, Value};

use policy_state::primitives::U256;

use super::params::param_action;
use super::FactCtx;
use super::FactError;

/// Parse a `U256` from a JSON value that may be a hex string (`"0x.."`), a
/// decimal string, or a JSON number. Reducer-supplied `live_inputs.reserveState`
/// amounts are not guaranteed to share the lowered-Cedar hex `amount` encoding,
/// so accept both forms (and a bare JSON integer) rather than assuming one.
fn u256_from_value(v: &Value, what: &str) -> Result<U256, FactError> {
    match v {
        Value::String(s) => {
            let s = s.trim();
            let parsed = s.strip_prefix("0x").map_or_else(
                || U256::from_str_radix(s, 10),
                |hex| U256::from_str_radix(hex, 16),
            );
            parsed.map_err(|e| FactError::BadParams(format!("`{what}` is not a U256: {e}")))
        }
        Value::Number(n) if n.is_u64() => Ok(U256::from(n.as_u64().unwrap_or(0))),
        _ => Err(FactError::BadParams(format!(
            "`{what}` is not a U256 (string or unsigned integer)"
        ))),
    }
}

/// Render `(used / cap)` as integer basis points (`used * 10_000 / cap`) using
/// U256 math, saturating at `10_000` when over cap. A zero cap means "no cap
/// configured": report a fully-used `10_000` for a positive numerator so a
/// `.greaterThan(threshold)` policy still trips, `0` for a zero numerator.
fn used_bp(used: U256, cap: U256) -> i64 {
    if cap.is_zero() {
        return if used.is_zero() { 0 } else { 10_000 };
    }
    let scaled = used.saturating_mul(U256::from(10_000u64)) / cap;
    let capped = scaled.min(U256::from(10_000u64));
    i64::try_from(capped).unwrap_or(10_000)
}

/// Dispatch a `reserve.*` enrichment fact against `ctx`.
///
/// FROZEN: one arm per sim-server `reserve.*` method in the catalog, plus a
/// catch-all. Do not edit this match when filling in bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] for an unregistered method, or whatever
/// error the per-method fn surfaces (currently [`FactError::NotImplemented`]
/// until the body is filled in).
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "reserve.is_siloed" => is_siloed(params, ctx),
        "reserve.isolation_debt_headroom_bps" => isolation_debt_headroom_bps(params, ctx),
        "reserve.cap_used_bp" => cap_used_bp(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// AAVE-10: whether the borrow target reserve is configured as siloed
/// (single-asset borrowing). The siloed flag is reserve-config metadata, NOT a
/// base context field, so it must come from a dedicated reserve-config read.
///
/// readKind: `direct`.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `venue`: String (required) — lending venue identifying the reserve
/// - `asset`: `AssetRef` (required) — `$.action.asset`
///
/// Catalog outputs:
/// - `isSiloed`: Bool — from `$.result.isSiloed`
///
/// State accessors the implementer should call:
/// - `WalletState.positions: Vec<Position>` — locate the lending venue's reserve
///   record for `asset` to read its siloed flag.
// STATE-WORKER ASK: needs reserve-config metadata accessor (siloed flag per (venue, asset)) — not surfaced in the Ground accessor list
// BLOCKED: no per-reserve "siloed" flag exists in the state map. `LendingAccount`
// carries only { market, collaterals, debts, emode, is_isolated, health_factor,
// ltv, liquidation_threshold } — `is_isolated` is the isolation-mode flag, NOT
// the siloed-borrowing flag (a distinct Aave reserve-config bit). No reserve-
// config accessor exists on WalletState. Cannot implement without fabricating.
fn is_siloed(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let _ = (params, ctx);
    Err(FactError::NotImplemented("reserve.is_siloed".into()))
}

/// AAVE-11: how close (basis points of the debt ceiling) the isolation-mode total
/// debt would be AFTER this borrow = `(isolationModeTotalDebt + borrow_usd) /
/// debtCeiling`. The ceiling and the protocol-global `isolationModeTotalDebt` are
/// reserve/protocol-global metadata, not base context fields.
///
/// readKind: `derived`.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `owner`: String (required) — `$.root.from`
/// - `venue`: String (required) — lending venue identifying the isolated reserve
/// - `action`: Action (required) — `$.action`; the proposed borrow whose USD
///   value is added to `isolationModeTotalDebt`
///
/// Catalog outputs:
/// - `headroomBps`: Long — from `$.result.headroomBps`
///
/// State accessors the implementer should call:
/// - `WalletState.positions: Vec<Position>` — read the lending position's
///   `is_isolated` flag (`positions(lending).data_json.is_isolated`).
/// - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` +
///   `TokenHolding.price_usd: Option<LiveField<Price>>` — price the borrow amount
///   (`action.amount`) in USD (`token_holdings.price_value`) for the numerator.
// STATE-WORKER ASK: needs protocol-global reserve metadata accessor (debtCeiling + isolationModeTotalDebt per (venue, reserve)) — not surfaced in the Ground accessor list
// BLOCKED: the headroom is `(isolationModeTotalDebt + borrow_usd) / debtCeiling`,
// but neither `isolationModeTotalDebt` nor `debtCeiling` exists anywhere in the
// state map. `LendingAccount` exposes only `is_isolated` (a bool flag), not the
// per-reserve debt ceiling nor the protocol-global isolation-mode total debt,
// and these are NOT carried on `action.live_inputs` per the catalog
// stateDependency. The numerator's borrow USD is computable, but with both the
// base (isolationModeTotalDebt) and denominator (debtCeiling) absent the ratio
// cannot be produced without fabricating two fields.
fn isolation_debt_headroom_bps(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let _ = (params, ctx);
    Err(FactError::NotImplemented(
        "reserve.isolation_debt_headroom_bps".into(),
    ))
}

/// AAVE-12: how full the reserve's supply/borrow cap would be AFTER this action,
/// in basis points = `(total + action.amount) / cap`. `cap_kind` selects supply
/// vs borrow. Servable from the action's own `live_inputs.reserveState`
/// (totalSupply / totalBorrow / supplyCap / borrowCap) + `action.amount` — no
/// wallet-state read.
///
/// readKind: `derived`.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `cap_kind`: String (required) — enum `supply | borrow`; which cap to measure
///   against
/// - `action`: Action (required) — `$.action`; the proposed supply/borrow whose
///   amount is added to the current total
///
/// Catalog outputs:
/// - `capUsedBp`: Long — from `$.result.capUsedBp`
///
/// State accessors the implementer should call:
/// - (none) — totals and caps ride on the action's
///   `live_inputs.reserveState.{totalSupply,totalBorrow,supplyCap,borrowCap}` and
///   the added amount on `action.amount`; this is a pure action-field derived read
///   with no wallet-state dependency.
fn cap_used_bp(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let cap_kind = super::params::param_str(params, "cap_kind")?;
    let (total_key, cap_key) = match cap_kind.as_str() {
        "supply" => ("totalSupply", "supplyCap"),
        "borrow" => ("totalBorrow", "borrowCap"),
        other => {
            return Err(FactError::BadParams(format!(
                "`cap_kind` is {other:?}, expected \"supply\" or \"borrow\""
            )))
        }
    };

    let action = param_action(params, "action")?;
    let reserve_state = action
        .get("live_inputs")
        .and_then(|li| li.get("reserveState"))
        .ok_or_else(|| {
            FactError::BadParams("missing `action.live_inputs.reserveState`".to_owned())
        })?;

    let total = u256_from_value(
        reserve_state
            .get(total_key)
            .ok_or_else(|| FactError::BadParams(format!("missing `reserveState.{total_key}`")))?,
        total_key,
    )?;
    let cap = u256_from_value(
        reserve_state
            .get(cap_key)
            .ok_or_else(|| FactError::BadParams(format!("missing `reserveState.{cap_key}`")))?,
        cap_key,
    )?;
    let amount = u256_from_value(
        action
            .get("amount")
            .ok_or_else(|| FactError::BadParams("missing `action.amount`".to_owned()))?,
        "amount",
    )?;

    let used = total.saturating_add(amount);
    Ok(json!({ "capUsedBp": used_bp(used, cap) }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use serde_json::json;

    use policy_state::{WalletId, WalletState};

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            "0x000000000000000000000000000000000000a01c"
                .parse()
                .unwrap(),
            [policy_state::primitives::ChainId::ethereum_mainnet()],
        ))
    }

    /// `cap_kind` + an action carrying `live_inputs.reserveState` totals/caps and
    /// the proposed `amount`. Amounts may be decimal or hex strings to exercise
    /// both branches of `u256_from_value`.
    fn cap_params(cap_kind: &str, total: &str, cap: &str, amount: &str) -> Value {
        let reserve_state = if cap_kind == "supply" {
            json!({ "totalSupply": total, "supplyCap": cap })
        } else {
            json!({ "totalBorrow": total, "borrowCap": cap })
        };
        json!({
            "chain_id": 1,
            "cap_kind": cap_kind,
            "action": {
                "amount": amount,
                "live_inputs": { "reserveState": reserve_state }
            }
        })
    }

    #[test]
    fn borrow_cap_used_bp_is_basis_points() {
        // total 800 + amount 100 = 900 of a 1000 cap → 9000 bp.
        let p = cap_params("borrow", "800", "1000", "100");
        let out = dispatch(
            "reserve.cap_used_bp",
            &p,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["capUsedBp"], json!(9000));
    }

    #[test]
    fn supply_cap_over_cap_saturates_at_10000() {
        // total 950 + amount 200 = 1150 over a 1000 cap → clamp to 10000 bp.
        let p = cap_params("supply", "950", "1000", "200");
        let out = dispatch(
            "reserve.cap_used_bp",
            &p,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["capUsedBp"], json!(10_000));
    }

    #[test]
    fn hex_encoded_amounts_parse() {
        // total 0x1f4 (500) + amount 0x64 (100) = 600 of 0x3e8 (1000) cap → 6000 bp.
        let p = cap_params("borrow", "0x1f4", "0x3e8", "0x64");
        let out = dispatch(
            "reserve.cap_used_bp",
            &p,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["capUsedBp"], json!(6000));
    }

    #[test]
    fn zero_cap_with_positive_use_reports_full() {
        let p = cap_params("borrow", "0", "0", "1");
        let out = dispatch(
            "reserve.cap_used_bp",
            &p,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["capUsedBp"], json!(10_000));
    }

    #[test]
    fn bad_cap_kind_is_bad_params() {
        let p = cap_params("collateral", "1", "1", "1");
        let err = dispatch(
            "reserve.cap_used_bp",
            &p,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn missing_reserve_state_is_bad_params() {
        let p = json!({ "cap_kind": "borrow", "action": { "amount": "1" } });
        let err = dispatch(
            "reserve.cap_used_bp",
            &p,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    // BLOCKED methods: no per-reserve siloed flag / debtCeiling /
    // isolationModeTotalDebt exists in the state map, so these stay
    // NotImplemented until a reserve-config accessor is surfaced.
    #[test]
    fn blocked_methods_are_not_implemented() {
        for method in ["reserve.is_siloed", "reserve.isolation_debt_headroom_bps"] {
            let err = dispatch(
                method,
                &json!({}),
                &FactCtx {
                    state: &empty_state(),
                },
            )
            .unwrap_err();
            assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
        }
    }
}
