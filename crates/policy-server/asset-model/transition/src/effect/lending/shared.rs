//! Shared math primitives for lending venues — index conversion, two-slope
//! borrow rate, supply rate, helpers used by Aave V2 / V3 / Spark / Compound /
//! Fluid / Morpho.
//!
//! Pure module — no side effects. Phase 2 approximation: when a per-venue
//! deployment does not yet expose a real interest index, callers feed
//! `ReserveState.total_supply` / `total_borrow` (present-value) directly and
//! we treat the index as `1`. The signature is conservative: when a real
//! index is plumbed through, swap the body for the index-scaled formula
//! without changing the call sites.

// Phase 2 stubs: some action reducers may not yet exercise every helper. The
// venue-specific dispatchers will wire these up; until then `dead_code` is
// the expected diagnostic.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, U256};

use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

/// Convert an asset amount into the equivalent scaled (receipt) balance
/// using a 1:1 index approximation.
///
/// Returns `Invariant` when the reserve is malformed (zero `total_supply`
/// but non-zero `total_borrow`, which `present_value_supply = supply - borrow`
/// would have underflowed).
pub(super) fn asset_to_scaled_balance(
    reserve: &ReserveState,
    asset_amount: U256,
    venue_tag: &'static str,
) -> ReducerResult<U256> {
    // Phase 2 approximation: index = 1. Validate the reserve first to surface
    // a degenerate state early (rather than silently returning 0).
    if reserve.total_supply < reserve.total_borrow {
        return Err(ReducerError::Invariant(format!(
            "{venue_tag} reserve invariant: total_supply {} < total_borrow {}",
            reserve.total_supply, reserve.total_borrow
        )));
    }
    Ok(asset_amount)
}

/// Inverse of [`asset_to_scaled_balance`] — identical body under the index=1
/// approximation, kept separate so future index work has matching shapes.
pub(super) fn scaled_balance_to_asset(
    reserve: &ReserveState,
    scaled_amount: U256,
    venue_tag: &'static str,
) -> ReducerResult<U256> {
    if reserve.total_supply < reserve.total_borrow {
        return Err(ReducerError::Invariant(format!(
            "{venue_tag} reserve invariant: total_supply {} < total_borrow {}",
            reserve.total_supply, reserve.total_borrow
        )));
    }
    Ok(scaled_amount)
}

/// Two-slope variable borrow rate (Aave-style); inputs all in basis points,
/// output in per-year `Decimal` (so `0.04` = 4 % APR).
///
/// Formula (matches Aave V3 `DefaultReserveInterestRateStrategyV2`):
///
/// ```text
///   U < U_opt:   r = r_base + slope1 * (U / U_opt)
///   U ≥ U_opt:   r = r_base + slope1 + slope2 * (U - U_opt) / (10_000 - U_opt)
/// ```
///
/// `utilization_bp` must satisfy `<= 10_000`. `optimal_bp` must satisfy
/// `0 < optimal_bp < 10_000` so the post-kink denominator stays positive.
pub(super) fn two_slope_borrow_apr(
    utilization_bp: u32,
    base_rate_bp: u32,
    slope1_bp: u32,
    slope2_bp: u32,
    optimal_bp: u32,
) -> Result<Decimal, String> {
    if utilization_bp > 10_000 {
        return Err(format!("utilization {utilization_bp} bp > 10000 (>100 %)"));
    }
    if optimal_bp == 0 || optimal_bp >= 10_000 {
        return Err(format!("optimal {optimal_bp} bp out of (0, 10000) range"));
    }

    let rate_bp: u64 = if utilization_bp <= optimal_bp {
        u64::from(base_rate_bp)
            + (u64::from(slope1_bp) * u64::from(utilization_bp)) / u64::from(optimal_bp)
    } else {
        let excess = u64::from(utilization_bp - optimal_bp);
        let span = u64::from(10_000 - optimal_bp);
        u64::from(base_rate_bp) + u64::from(slope1_bp) + (u64::from(slope2_bp) * excess) / span
    };

    Ok(bp_to_decimal(rate_bp))
}

/// Supply (deposit) rate — borrow rate × utilization × (1 - `reserve_factor`).
///
/// `reserve_factor_bp` is the share that goes to protocol reserves
/// (typically 10-20 %); the remainder flows to suppliers proportional to the
/// pool's utilization. All inputs in basis points; output is per-year
/// `Decimal`.
pub(super) fn supply_apr_from_borrow(
    borrow_apr: &Decimal,
    utilization_bp: u32,
    reserve_factor_bp: u32,
) -> Result<Decimal, String> {
    if utilization_bp > 10_000 {
        return Err(format!("utilization {utilization_bp} bp > 10000"));
    }
    if reserve_factor_bp > 10_000 {
        return Err(format!("reserve_factor {reserve_factor_bp} bp > 10000"));
    }
    let borrow_dec = parse_decimal(borrow_apr)?;
    let util_factor = bp_to_rust_decimal(utilization_bp);
    let rf_factor = bp_to_rust_decimal(10_000 - reserve_factor_bp);
    let supply_dec = borrow_dec * util_factor * rf_factor;
    Ok(rust_decimal_to_state(supply_dec))
}

/// Render basis points (e.g. `400` = 4 %) as a `Decimal` ("0.04").
fn bp_to_decimal(bp: u64) -> Decimal {
    let int = bp / 10_000;
    let frac = bp % 10_000;
    if frac == 0 {
        Decimal::new(int.to_string())
    } else {
        // Trim trailing zeros on the fractional side for human-friendly
        // round-trip ("400" → "0.04", not "0.0400").
        let mut frac_str = format!("{frac:04}");
        while frac_str.ends_with('0') {
            frac_str.pop();
        }
        Decimal::new(format!("{int}.{frac_str}"))
    }
}

/// Internal conversion: basis points → `rust_decimal::Decimal`.
fn bp_to_rust_decimal(bp: u32) -> rust_decimal::Decimal {
    rust_decimal::Decimal::new(i64::from(bp), 4)
}

/// Internal parse helper.
fn parse_decimal(d: &Decimal) -> Result<rust_decimal::Decimal, String> {
    use std::str::FromStr;
    rust_decimal::Decimal::from_str(d.as_str())
        .map_err(|e| format!("invalid Decimal {:?}: {e}", d.as_str()))
}

/// Internal render helper — canonical (no trailing-zero) string form.
fn rust_decimal_to_state(d: rust_decimal::Decimal) -> Decimal {
    Decimal::new(d.normalize().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bp_to_decimal_pure_integer() {
        assert_eq!(bp_to_decimal(10_000).as_str(), "1");
        assert_eq!(bp_to_decimal(20_000).as_str(), "2");
    }

    #[test]
    fn bp_to_decimal_fractional() {
        assert_eq!(bp_to_decimal(400).as_str(), "0.04");
        assert_eq!(bp_to_decimal(50).as_str(), "0.005");
        assert_eq!(bp_to_decimal(1).as_str(), "0.0001");
    }

    #[test]
    fn two_slope_zero_utilization() {
        let r = two_slope_borrow_apr(0, 0, 400, 6_000, 8_000).unwrap();
        assert_eq!(r.as_str(), "0");
    }

    #[test]
    fn two_slope_max_utilization() {
        // At 100 % utilization the rate is `base + slope1 + slope2`.
        let r = two_slope_borrow_apr(10_000, 0, 400, 6_000, 8_000).unwrap();
        assert_eq!(r.as_str(), "0.64");
    }

    #[test]
    fn supply_rate_below_kink_matches_share() {
        // borrow 4 %, util 40 %, reserve_factor 10 % → 0.04 * 0.4 * 0.9 = 0.0144.
        let borrow = Decimal::new("0.04");
        let r = supply_apr_from_borrow(&borrow, 4_000, 1_000).unwrap();
        assert_eq!(r.as_str(), "0.0144");
    }
}
