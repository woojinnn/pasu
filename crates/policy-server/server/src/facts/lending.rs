//! `lending.*` — main semantic-vocab enrichment namespace (option-2 convergence).
//!
//! Hosts the main-vocab `lending.health_factor` method. Its computation is the
//! shared snapshot-based [`super::position::post_action_hf`] (reads the already
//! lowered `action.live_inputs` snapshot — `userStateBefore` + `reserveState` —
//! per ADR-010 snapshot-first; NO synced-state / per-asset-LT substrate). The
//! only convergence delta vs the legacy `position.health_factor_after` is the
//! method name and the result key (`postActionHf`), which a snapshot-params
//! manifest projects onto `context.custom.postActionHf`.

use serde_json::{json, Value};

use super::FactCtx;
use super::FactError;

/// Dispatch a `lending.*` method to its fact implementation.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "lending.health_factor" => health_factor(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// main-vocab `lending.health_factor` → result key `postActionHf` (main manifest
/// projects `$.result.postActionHf` onto `context.custom.postActionHf`).
///
/// Same value as `position.health_factor_after`; the convergence is purely the
/// method/result naming, so the formula lives in one place
/// ([`super::position::post_action_hf`]).
fn health_factor(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    Ok(json!({ "postActionHf": super::position::post_action_hf(params, ctx)? }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use policy_state::primitives::{ChainId, U256};
    use policy_state::{WalletId, WalletState};

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            "0x000000000000000000000000000000000000a01c".parse().unwrap(),
            [ChainId::ethereum_mainnet()],
        ))
    }

    /// Snapshot-based action param (the lowered Aave borrow context fields the
    /// fact reads): `{chain_id, owner, venue, action: {..userStateBefore..}}`.
    fn params(amount_hex: &str, total_debt_hex: &str, hf: &str) -> Value {
        json!({
            "chain_id": "eip155:1",
            "owner": "0x000000000000000000000000000000000000a01c",
            "venue": "aave-v3",
            "action": {
                "asset": { "key": { "standard": "erc20", "chain": "eip155:1", "address": USDC } },
                "amount": amount_hex,
                "assetPriceUsd": "1.00",
                "reserveState": { "liquidationThresholdBp": 7400 },
                "userStateBefore": {
                    "healthFactor": hf,
                    "totalCollatUsd": format!("{:#x}", U256::from(50_000_000_000u64)),
                    "totalDebtUsd": total_debt_hex,
                }
            }
        })
    }

    #[test]
    fn lending_health_factor_projects_post_action_hf() {
        // No-debt-after path (amount 0, debt 0) → mirrors State₁ HF, 4-dp
        // normalized. Proves the main-vocab method routes and emits `postActionHf`
        // (not `healthFactor`).
        let state = empty_state();
        let zero = format!("{:#x}", U256::ZERO);
        let out = dispatch(
            "lending.health_factor",
            &params(&zero, &zero, "1.85"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["postActionHf"], json!("1.8500"));
        assert!(out.get("healthFactor").is_none(), "must use main result key");
    }

    #[test]
    fn lending_health_factor_value_matches_position_fact() {
        // Same inputs through both the legacy `position.health_factor_after`
        // (healthFactor) and the converged `lending.health_factor` (postActionHf)
        // must yield the identical decimal — single shared formula.
        let state = empty_state();
        let zero = format!("{:#x}", U256::ZERO);
        let p = params(&zero, &zero, "2.40");

        let lending = dispatch("lending.health_factor", &p, &FactCtx { state: &state }).unwrap();
        let position = super::super::position::dispatch(
            "position.health_factor_after",
            &p,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(lending["postActionHf"], position["healthFactor"]);
    }

    #[test]
    fn unknown_lending_method_is_unknown() {
        let state = empty_state();
        let err = dispatch("lending.nope", &json!({}), &FactCtx { state: &state }).unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }
}
