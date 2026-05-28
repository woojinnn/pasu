//! Spark Protocol venue math — Aave V3 fork; math identical, parameters and
//! integrations differ.
//!
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::Spark`. Not a `Reducer` impl.
//!
//! ## Fork relationship
//!
//! Spark Lend is a Maker-governed Aave V3 fork — the [`spark-lend` repo](https://github.com/marsfoundation/spark-lend)
//! tracks Aave V3 commit-for-commit on the math libraries, overriding only
//! parameters (e.g. DAI uses a flat `slope1 = 0`, no kink) and adding
//! integrations (DSR-fed Direct Deposit Module).
//!
//! The body therefore delegates to the same shared helpers as V3 with
//! Spark-specific defaults.

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
    shared::asset_to_scaled_balance(reserve, asset_amount, "spark")
}

/// Inverse of [`asset_to_atokens`].
pub(super) fn atokens_to_asset(
    _state: &WalletState,
    _ctx: &EvalContext,
    reserve: &ReserveState,
    atoken_amount: U256,
) -> ReducerResult<U256> {
    shared::scaled_balance_to_asset(reserve, atoken_amount, "spark")
}

/// Compute the per-year (APR) borrow rate on a reserve given its current
/// utilization.
///
/// Spark's parameter sheet differs from Aave V3 — DAI / sDAI reserves run a
/// `slope1 = 0` "passthrough" model fed from the DAI Savings Rate, while
/// the rest of the reserves use Aave V3's defaults. We use the Aave V3
/// defaults here as the conservative baseline; an explicit per-asset
/// Spark-strategy lookup will land when the sync orchestrator wires the
/// strategy address into `ReserveState`.
pub(super) fn current_borrow_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        SPARK_BASE_RATE_BP,
        SPARK_SLOPE1_BP,
        SPARK_SLOPE2_BP,
        SPARK_OPTIMAL_UTILIZATION_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("spark rate: {msg}")))
}

/// Spark default base rate (per year, basis points).
const SPARK_BASE_RATE_BP: u32 = 0;

/// Spark default `slope1` — matches Aave V3 USDC parameters.
const SPARK_SLOPE1_BP: u32 = 400;

/// Spark default `slope2` — matches Aave V3 USDC parameters.
const SPARK_SLOPE2_BP: u32 = 6_000;

/// Spark default `optimalUsageRatio` — `80 %`.
const SPARK_OPTIMAL_UTILIZATION_BP: u32 = 8_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn reserve_with(
        total_supply: u128,
        total_borrow: u128,
        utilization_bp: u32,
    ) -> ReserveState {
        ReserveState {
            total_supply: U256::from(total_supply),
            total_borrow: U256::from(total_borrow),
            utilization_bp,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 7_400,
            liquidation_threshold_bp: 7_600,
            liquidation_bonus_bp: 700,
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

    /// Identical math to Aave V3 under the index=1 approximation.
    #[test]
    fn asset_to_atokens_one_to_one() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let out = asset_to_atokens(&empty_state(), &dummy_ctx(), &r, U256::from(123u64)).unwrap();
        assert_eq!(out, U256::from(123u64));
    }

    /// At `U = 80 %` Spark's default rate matches Aave V3's `4 %`.
    #[test]
    fn borrow_rate_at_kink_matches_aave_v3_defaults() {
        let r = reserve_with(10_000, 8_000, 8_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.04");
    }

    /// Out-of-range utilization (>100 %) is rejected.
    #[test]
    fn borrow_rate_oversaturated_errors() {
        let r = reserve_with(10_000, 9_000, 10_500);
        let err = current_borrow_rate(&r).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
