//! Compound V2 venue math — classic `cToken` exchange-rate model.
//!
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::CompoundV2`. Not a `Reducer` impl.
//!
//! Suppliers receive `cToken`s whose exchange rate against the underlying
//! grows over time as interest accrues to the market.
//!
//! ## Exchange-rate approximation
//!
//! `CToken.sol::exchangeRateStoredInternal` defines:
//!
//! ```text
//!   exchangeRate = (totalCash + totalBorrows - totalReserves) / totalSupply
//! ```
//!
//! when `totalSupply > 0`. The current `ReserveState` exposes the present-
//! value `total_supply` (asset units) and `total_borrow`. We approximate
//! `totalCash + totalBorrows - totalReserves == total_supply` (i.e. assume
//! the supply tally is already the cash-side total) so under Phase 2 the
//! cToken / asset ratio remains 1:1. The shape lets us swap in the actual
//! `(cash, borrows, reserves, cTokenSupply)` tuple once the sync orchestrator
//! plumbs them through.
//!
//! ## Rate model
//!
//! Two-slope Jump-rate per Compound V2's `JumpRateModelV2`:
//!
//! ```text
//!   below kink: r = base + util * multiplier
//!   above kink: r = base + kink * multiplier + (util - kink) * jumpMultiplier
//! ```
//!
//! Rates are stored per-block on-chain (`BLOCKS_PER_YEAR = 2_628_000`); the
//! `Decimal` we return uses per-year units (APR), matching every other
//! venue's `current_borrow_rate`. Compound V2 defaults track mainnet cUSDC.

// Phase 2 stubs: per-action wiring (`supply.rs`, `borrow.rs`, …) lands in a
// later commit in the same batch.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, U256};

use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

use super::shared;

/// Convert an underlying asset amount into the equivalent `cToken` amount
/// using the current exchange rate.
pub(super) fn underlying_to_ctoken(
    reserve: &ReserveState,
    underlying_amount: U256,
) -> ReducerResult<U256> {
    shared::asset_to_scaled_balance(reserve, underlying_amount, "compound_v2")
}

/// Inverse of [`underlying_to_ctoken`].
pub(super) fn ctoken_to_underlying(
    reserve: &ReserveState,
    ctoken_amount: U256,
) -> ReducerResult<U256> {
    shared::scaled_balance_to_asset(reserve, ctoken_amount, "compound_v2")
}

/// Compute the per-year (APR) borrow rate on a market given its current
/// utilization.
///
/// On-chain Compound V2 stores rates per-block (`BLOCKS_PER_YEAR =
/// 2_628_000`); we return APR to stay consistent with every other venue.
pub(super) fn current_borrow_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        COMPOUND_V2_BASE_RATE_BP,
        COMPOUND_V2_MULTIPLIER_BP,
        COMPOUND_V2_JUMP_MULTIPLIER_BP,
        COMPOUND_V2_KINK_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("compound_v2 rate: {msg}")))
}

/// Compute the per-year (APR) supply rate on a market given its current
/// utilization and reserve factor.
pub(super) fn current_supply_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    let borrow = current_borrow_rate(reserve)?;
    shared::supply_apr_from_borrow(&borrow, reserve.utilization_bp, reserve.reserve_factor_bp)
        .map_err(|msg| ReducerError::Invariant(format!("compound_v2 supply rate: {msg}")))
}

/// Compound V2 default base rate (per year, basis points). cUSDC = 0 %.
const COMPOUND_V2_BASE_RATE_BP: u32 = 0;

/// Compound V2 `multiplierPerBlock` * `BLOCKS_PER_YEAR` for cUSDC ≈ 5 %.
const COMPOUND_V2_MULTIPLIER_BP: u32 = 500;

/// Compound V2 `jumpMultiplierPerBlock` * `BLOCKS_PER_YEAR` for cUSDC ≈
/// 109 %.
const COMPOUND_V2_JUMP_MULTIPLIER_BP: u32 = 10_900;

/// Compound V2 `kink` for cUSDC = 80 %.
const COMPOUND_V2_KINK_BP: u32 = 8_000;

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
            ltv_bp: 7_500,
            liquidation_threshold_bp: 8_000,
            liquidation_bonus_bp: 800,
            reserve_factor_bp: 1_000,
            is_frozen: false,
            is_paused: false,
        }
    }

    /// `underlying_to_ctoken` honours the index=1 approximation.
    #[test]
    fn underlying_to_ctoken_one_to_one() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let out = underlying_to_ctoken(&r, U256::from(2_500u64)).unwrap();
        assert_eq!(out, U256::from(2_500u64));
    }

    /// Round-trip preserves the asset amount under the index=1 approximation.
    #[test]
    fn underlying_ctoken_round_trip() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let in_amt = U256::from(987_654u64);
        let ctok = underlying_to_ctoken(&r, in_amt).unwrap();
        let out = ctoken_to_underlying(&r, ctok).unwrap();
        assert_eq!(out, in_amt);
    }

    /// At the kink (`U = 80 %`) the rate equals `base + slope1 * (U/optimal)
    /// = 0 + 5 % * 1 = 5 %`.
    #[test]
    fn borrow_rate_at_kink() {
        let r = reserve_with(10_000, 8_000, 8_000);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.05");
    }

    /// Above kink: jump multiplier engages.
    /// At `U = 90 %`: `base + multiplier + jumpMultiplier * (10/20) =
    /// 0 + 0.05 + 1.09 * 0.5 = 0.05 + 0.545 = 0.595`.
    /// (using `multiplier = 0.05` since `multiplier_bp = 500` represents the
    /// slope1 value at kink — see Compound V2 docs.)
    #[test]
    fn borrow_rate_above_kink_engages_jump() {
        let r = reserve_with(10_000, 9_000, 9_000);
        let rate = current_borrow_rate(&r).unwrap();
        // 0 + 500_bp_slope1 + (10_900_bp * 1000_excess / 2000_span) = 500 + 5450 = 5950 bp = 0.595
        assert_eq!(rate.as_str(), "0.595");
    }

    /// Supply rate is borrow * util * (1 - `reserve_factor`).
    /// At `U = 50 %`: borrow = `base + slope1 * (U / optimal) =
    /// 0 + 500 * 5000 / 8000 = 312 bp` (integer division floors).
    /// supply = `0.0312 * 0.5 * 0.9 = 0.01404`.
    #[test]
    fn supply_rate_below_kink() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let s = current_supply_rate(&r).unwrap();
        assert_eq!(s.as_str(), "0.01404");
    }

    /// Degenerate reserve rejected.
    #[test]
    fn underlying_degenerate_reserve_errors() {
        let r = reserve_with(100, 500, 5_000);
        let err = underlying_to_ctoken(&r, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
