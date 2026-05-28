//! Compound V3 (`Comet`) venue math — single base asset per market.
//!
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::CompoundV3`. Not a `Reducer` impl.
//!
//! In Compound V3 each market has a single base asset; suppliers earn interest
//! on the base while collateral assets do not. Both supply and borrow balances
//! are tracked as principal amounts that accrue via per-market indices.
//!
//! ## Principal / present value
//!
//! `Comet.sol` carries two indices — `baseSupplyIndex` and `baseBorrowIndex`,
//! both `BASE_INDEX_SCALE = 1e15`-denominated. The conversion is:
//!
//! ```text
//!   presentValueSupply = (principal * baseSupplyIndex) / BASE_INDEX_SCALE
//!   principalValueSupply = (present  * BASE_INDEX_SCALE) / baseSupplyIndex
//! ```
//!
//! With borrow as the mirror. `ReserveState` does not yet carry the indices,
//! so we approximate them at 1 (i.e. `principal == present`). When the sync
//! orchestrator feeds explicit indices the body switches to the closed form.
//!
//! ## Rate model
//!
//! Two-slope per-second model. We surface the per-year APR `Decimal` for
//! consistency with every other venue; per-second arithmetic is left to the
//! sync orchestrator. Defaults track mainnet `cUSDCv3`.

// Phase 2 stubs: per-action wiring lands in a later commit in the same batch.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, U256};

use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

use super::shared;

/// Scale a stored principal balance by the current supply/borrow index to
/// obtain the present-value amount denominated in the base asset.
pub(super) fn principal_to_present_value(
    reserve: &ReserveState,
    principal_amount: U256,
) -> ReducerResult<U256> {
    shared::asset_to_scaled_balance(reserve, principal_amount, "compound_v3")
}

/// Inverse of [`principal_to_present_value`].
pub(super) fn present_value_to_principal(
    reserve: &ReserveState,
    present_amount: U256,
) -> ReducerResult<U256> {
    shared::scaled_balance_to_asset(reserve, present_amount, "compound_v3")
}

/// Compute the per-year (APR) supply rate on the base asset given current
/// utilization.
pub(super) fn current_supply_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    let borrow = current_borrow_rate(reserve)?;
    shared::supply_apr_from_borrow(&borrow, reserve.utilization_bp, reserve.reserve_factor_bp)
        .map_err(|msg| ReducerError::Invariant(format!("compound_v3 supply rate: {msg}")))
}

/// Compute the per-year (APR) borrow rate on the base asset given current
/// utilization.
pub(super) fn current_borrow_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        COMPOUND_V3_BASE_RATE_BP,
        COMPOUND_V3_SLOPE_LOW_BP,
        COMPOUND_V3_SLOPE_HIGH_BP,
        COMPOUND_V3_KINK_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("compound_v3 rate: {msg}")))
}

/// Compound V3 base borrow rate per year (basis points). `cUSDCv3` mainnet
/// = ~`1.5 %` (perBlock × `SECONDS_PER_YEAR`).
const COMPOUND_V3_BASE_RATE_BP: u32 = 150;

/// Compound V3 below-kink slope. `cUSDCv3` mainnet ≈ `3.5 %`.
const COMPOUND_V3_SLOPE_LOW_BP: u32 = 350;

/// Compound V3 above-kink slope. `cUSDCv3` mainnet ≈ `170 %`.
const COMPOUND_V3_SLOPE_HIGH_BP: u32 = 17_000;

/// Compound V3 `kink` utilization. `cUSDCv3` mainnet = `93 %`.
const COMPOUND_V3_KINK_BP: u32 = 9_300;

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
            ltv_bp: 8_500,
            liquidation_threshold_bp: 8_800,
            liquidation_bonus_bp: 800,
            reserve_factor_bp: 1_000,
            is_frozen: false,
            is_paused: false,
        }
    }

    /// Principal-to-present round-trip preserves the value under index=1.
    #[test]
    fn principal_present_round_trip() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let p = U256::from(1_234_567u64);
        let pv = principal_to_present_value(&r, p).unwrap();
        let back = present_value_to_principal(&r, pv).unwrap();
        assert_eq!(back, p);
    }

    /// At kink (`U = 93 %`) the rate equals `base + slope1 = 1.5 % + 3.5 % = 5 %`.
    #[test]
    fn borrow_rate_at_kink() {
        let r = reserve_with(10_000, 9_300, 9_300);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.05");
    }

    /// At `U = 0 %` the rate is `base = 1.5 %`.
    #[test]
    fn borrow_rate_zero_utilization() {
        let r = reserve_with(10_000, 0, 0);
        let rate = current_borrow_rate(&r).unwrap();
        assert_eq!(rate.as_str(), "0.015");
    }

    /// Supply rate matches borrow * util * (1 - `reserve_factor`).
    /// `U = 93 %, borrow = 5 %, reserve_factor = 10 %`:
    /// `0.05 * 0.93 * 0.9 = 0.04185`.
    #[test]
    fn supply_rate_at_kink() {
        let r = reserve_with(10_000, 9_300, 9_300);
        let s = current_supply_rate(&r).unwrap();
        assert_eq!(s.as_str(), "0.04185");
    }

    /// Out-of-range utilization rejected.
    #[test]
    fn borrow_rate_oversaturated_errors() {
        let r = reserve_with(10_000, 9_000, 10_500);
        let err = current_borrow_rate(&r).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
