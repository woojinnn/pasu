//! Aave V3 venue math — interest index, aToken ratio, health-factor recompute.
//!
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::AaveV3`. Not a `Reducer` impl.
//!
//! ## Index approximation
//!
//! In a live Aave V3 deployment each reserve carries a `liquidityIndex` (RAY-scaled,
//! `1e27`) advanced by [`getNormalizedIncome`](https://github.com/aave-dao/aave-v3-origin/blob/main/src/contracts/protocol/libraries/logic/ReserveLogic.sol)
//! every interaction. The current `ReserveState` schema only carries the
//! present-value `total_supply` and `total_borrow`, so we approximate the
//! liquidity index as `1` (i.e. the present-value supply *is* the scaled aToken
//! supply). When the sync orchestrator starts feeding an explicit index this
//! file is the single point that needs updating.
//!
//! ## Rate model
//!
//! Two-slope variable borrow rate, matching [`DefaultReserveInterestRateStrategyV2`](https://github.com/aave-dao/aave-v3-origin/tree/main/src/contracts/misc):
//!
//! ```text
//!   U < U_optimal:   r = r_base + slope1 * (U / U_optimal)
//!   U ≥ U_optimal:   r = r_base + slope1 + slope2 * (U - U_optimal) / (1 - U_optimal)
//! ```
//!
//! Aave V3 hard-codes per-asset `optimalUsageRatio`; absent a real strategy
//! state we use defaults that match the most common Aave V3 reserves:
//! `r_base = 0`, `slope1 = 4 %`, `slope2 = 60 %`, `U_optimal = 80 %`.

// Phase 2 stubs: per-action wiring (`supply.rs`, `borrow.rs`, …) lands in a
// later commit in the same batch. Until every venue path consumes the
// per-fn dispatchers `dead_code` is the expected diagnostic.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, U256};
use simulation_state::{EvalContext, WalletState};

use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

use super::shared;

/// Convert an asset amount into the equivalent `aToken` amount using the
/// current liquidity index.
///
/// Phase 2 approximation — uses the `(present_value, scaled_supply)` ratio
/// the `ReserveState` exposes. When a real `liquidityIndex` is plumbed
/// through, swap the body for `asset_amount * RAY / liquidity_index`.
pub(super) fn asset_to_atokens(
    _state: &WalletState,
    _ctx: &EvalContext,
    reserve: &ReserveState,
    asset_amount: U256,
) -> ReducerResult<U256> {
    shared::asset_to_scaled_balance(reserve, asset_amount, "aave_v3")
}

/// Inverse of [`asset_to_atokens`].
pub(super) fn atokens_to_asset(
    _state: &WalletState,
    _ctx: &EvalContext,
    reserve: &ReserveState,
    atoken_amount: U256,
) -> ReducerResult<U256> {
    shared::scaled_balance_to_asset(reserve, atoken_amount, "aave_v3")
}

/// Compute the per-second borrow rate on a reserve given its current
/// utilization.
///
/// Returns the rate as a `Decimal` in **per-year (APR) units** — divide by
/// `SECONDS_PER_YEAR` at use-site if a per-second number is needed. Matches
/// the convention used by `BorrowLiveInputs::current_borrow_rate` elsewhere.
pub(super) fn current_borrow_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        AAVE_V3_BASE_RATE_BP,
        AAVE_V3_SLOPE1_BP,
        AAVE_V3_SLOPE2_BP,
        AAVE_V3_OPTIMAL_UTILIZATION_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("aave_v3 rate: {msg}")))
}

/// Aave V3 default base variable borrow rate (per year, basis points).
/// Most reserves run at 0 %; deviations come from the per-asset strategy
/// state which is not yet wired into `ReserveState`.
const AAVE_V3_BASE_RATE_BP: u32 = 0;

/// Aave V3 default `slope1` (per-year basis points). `4 %` matches USDC /
/// USDT / DAI on mainnet.
const AAVE_V3_SLOPE1_BP: u32 = 400;

/// Aave V3 default `slope2` (per-year basis points). `60 %` is the post-kink
/// penalty for over-utilised reserves.
const AAVE_V3_SLOPE2_BP: u32 = 6_000;

/// Aave V3 default `optimalUsageRatio` — `80 %`. Hard kink point above
/// which the punitive `slope2` engages.
const AAVE_V3_OPTIMAL_UTILIZATION_BP: u32 = 8_000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::ReserveState;

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

    /// Happy path: `asset_amount = 1_000` against a 1:1 reserve
    /// (`total_supply == present-value supply`) → `aToken_amount = 1_000`.
    #[test]
    fn asset_to_atokens_one_to_one() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let out = asset_to_atokens(&empty_state(), &dummy_ctx(), &r, U256::from(500u64)).unwrap();
        // Phase 2 approximation: liquidity index = 1, so 500 in → 500 out.
        assert_eq!(out, U256::from(500u64));
    }

    /// Inverse round-trip: `atokens_to_asset(asset_to_atokens(x)) == x`.
    #[test]
    fn asset_atoken_round_trip() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let amt = U256::from(123_456u64);
        let scaled = asset_to_atokens(&empty_state(), &dummy_ctx(), &r, amt).unwrap();
        let back = atokens_to_asset(&empty_state(), &dummy_ctx(), &r, scaled).unwrap();
        assert_eq!(back, amt);
    }

    /// Below-optimal utilization (`U = 40 %`) gives `slope1 * (40/80) = 2 %`.
    /// `r_base = 0` → final rate `0.02`.
    #[test]
    fn borrow_rate_below_kink() {
        let r = reserve_with(10_000, 4_000, 4_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.02");
    }

    /// At the kink (`U = 80 %`) the rate equals `r_base + slope1 = 4 %`.
    #[test]
    fn borrow_rate_at_kink() {
        let r = reserve_with(10_000, 8_000, 8_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.04");
    }

    /// Above the kink (`U = 90 %`) adds `slope2 * (10/20) = 30 %` → `34 %`.
    #[test]
    fn borrow_rate_above_kink() {
        let r = reserve_with(10_000, 9_000, 9_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.34");
    }

    /// Out-of-range utilization (>100 %) is rejected as an invariant.
    #[test]
    fn borrow_rate_oversaturated_utilization_errors() {
        let r = reserve_with(10_000, 9_000, 10_500);
        let err = current_borrow_rate(&r).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
