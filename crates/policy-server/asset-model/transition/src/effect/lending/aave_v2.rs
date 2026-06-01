//! Aave V2 venue math — predecessor of Aave V3; simpler interest model with
//! `Stable` debt support intact.
//!
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::AaveV2`. Not a `Reducer` impl.
//!
//! ## Index approximation
//!
//! Mirrors the V3 file: in a live deployment each reserve carries a
//! `liquidityIndex` (RAY = `1e27`); the present `ReserveState` carries only
//! present-value totals, so we approximate the index as `1` until the sync
//! orchestrator feeds an explicit value.
//!
//! ## Rate model
//!
//! Same two-slope shape as V3 but with V2's slightly more aggressive
//! defaults (`slope2 = 75 %`, `U_optimal = 80 %`) matching mainnet V2 USDC.

// Phase 2 stubs: per-action wiring lands in a later commit in the same batch.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, U256};
use simulation_state::{EvalContext, WalletState};

use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

use super::shared;

/// Convert an asset amount into the equivalent `aToken` amount using the
/// current liquidity index.
pub(super) fn asset_to_atokens(
    _state: &WalletState,
    _ctx: &EvalContext,
    reserve: &ReserveState,
    asset_amount: U256,
) -> ReducerResult<U256> {
    shared::asset_to_scaled_balance(reserve, asset_amount, "aave_v2")
}

/// Inverse of [`asset_to_atokens`].
pub(super) fn atokens_to_asset(
    _state: &WalletState,
    _ctx: &EvalContext,
    reserve: &ReserveState,
    atoken_amount: U256,
) -> ReducerResult<U256> {
    shared::scaled_balance_to_asset(reserve, atoken_amount, "aave_v2")
}

/// Compute the per-year (APR) borrow rate on a reserve given its current
/// utilization. Same convention as V3 — divide by `SECONDS_PER_YEAR` for a
/// per-second number.
pub(super) fn current_borrow_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        AAVE_V2_BASE_RATE_BP,
        AAVE_V2_SLOPE1_BP,
        AAVE_V2_SLOPE2_BP,
        AAVE_V2_OPTIMAL_UTILIZATION_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("aave_v2 rate: {msg}")))
}

/// Aave V2 base variable borrow rate (per year, basis points).
const AAVE_V2_BASE_RATE_BP: u32 = 0;

/// Aave V2 `slope1` — `4 %`.
const AAVE_V2_SLOPE1_BP: u32 = 400;

/// Aave V2 `slope2` — `75 %`. Pre-V3 V2 reserves used a steeper post-kink
/// penalty (cf. mainnet aUSDC at the time of the V3 cutover).
const AAVE_V2_SLOPE2_BP: u32 = 7_500;

/// Aave V2 `optimalUsageRatio` — `80 %`.
const AAVE_V2_OPTIMAL_UTILIZATION_BP: u32 = 8_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn reserve_with(total_supply: u128, total_borrow: u128, utilization_bp: u32) -> ReserveState {
        ReserveState {
            total_supply: U256::from(total_supply),
            total_borrow: U256::from(total_borrow),
            utilization_bp,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 8_000,
            liquidation_threshold_bp: 8_250,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1_000,
            is_frozen: false,
            is_paused: false,
        }
    }

    fn dummy_ctx() -> EvalContext {
        use simulation_state::eval_context::RequestKind;
        use simulation_state::primitives::{ChainId, Time};
        EvalContext::new(
            ChainId::ethereum_mainnet(),
            Time::from_unix(1_738_000_000),
            RequestKind::Transaction,
        )
    }

    fn empty_state() -> WalletState {
        use simulation_state::primitives::{Address, ChainId};
        use simulation_state::wallet::WalletId;
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            [ChainId::ethereum_mainnet()],
        ))
    }

    /// One-to-one ratio identical to V3 under the index=1 approximation.
    #[test]
    fn asset_to_atokens_one_to_one() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let out = asset_to_atokens(&empty_state(), &dummy_ctx(), &r, U256::from(750u64)).unwrap();
        assert_eq!(out, U256::from(750u64));
    }

    /// At the kink V2's rate equals `slope1 = 4 %`.
    #[test]
    fn borrow_rate_at_kink() {
        let r = reserve_with(10_000, 8_000, 8_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.04");
    }

    /// At max utilization V2's rate equals `base + slope1 + slope2 = 79 %`.
    #[test]
    fn borrow_rate_max_utilization() {
        let r = reserve_with(10_000, 10_000, 10_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.79");
    }

    /// Degenerate reserve (`total_borrow > total_supply`) → invariant.
    #[test]
    fn asset_to_atokens_reserve_invariant_errors() {
        let r = reserve_with(100, 500, 5_000);
        let err = asset_to_atokens(&empty_state(), &dummy_ctx(), &r, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
