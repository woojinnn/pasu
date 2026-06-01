//! Morpho Optimizer venue math — peer-to-peer layer on top of Aave / Compound.
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::MorphoOptimizer`. Not a `Reducer` impl.
//! Sits on top of Aave / Compound; rates blend p2p and pool. When matched p2p
//! liquidity is unavailable, positions fall through to the underlying pool at
//! that pool's prevailing rate.
//! ## P2P blend
//! Optimizer's [docs](https://docs.morpho.org/optimizers/concepts) describe the
//! blended rate as a midpoint between the underlying pool's supply and borrow
//! rates, weighted by the p2p-matched fraction:
//! ```text
//!   p2p_rate     = (pool_supply + pool_borrow) / 2
//!   p2p_supply   = blend(p2p_rate, pool_supply, supply_match_ratio)
//!   p2p_borrow   = blend(p2p_rate, pool_borrow, borrow_match_ratio)
//! ```
//! The `match_ratio` is supplied by the optimizer market state. Absent that
//! state we use the reserve's `utilization_bp` as a conservative proxy — at
//! high utilization more of the wallet's position sits in the pool fallback,
//! at low utilization the p2p mid dominates.

#![allow(dead_code)]

use policy_state::primitives::{Decimal, U256};

// `ReserveState` is reused here as a stand-in until an Optimizer-specific
use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

use super::shared;

/// Optimizer uses the underlying Aave pool's strategy parameters; the
/// blended `p2p_rate` is computed against those rather than a separate
/// curve. We reuse the Aave V3 defaults exposed by the `shared` module.
const POOL_BASE_RATE_BP: u32 = 0;
/// Underlying pool below-kink slope (Aave V3 default).
const POOL_SLOPE1_BP: u32 = 400;
/// Underlying pool above-kink slope (Aave V3 default).
const POOL_SLOPE2_BP: u32 = 6_000;
/// Underlying pool kink (Aave V3 default).
const POOL_OPTIMAL_BP: u32 = 8_000;

/// Compute the blended p2p supply rate given the p2p index and the current
/// pool supply rate fallback.
pub(super) fn p2p_supply_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    let pool_borrow = shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        POOL_BASE_RATE_BP,
        POOL_SLOPE1_BP,
        POOL_SLOPE2_BP,
        POOL_OPTIMAL_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("morpho_optimizer pool rate: {msg}")))?;
    let pool_supply = shared::supply_apr_from_borrow(
        &pool_borrow,
        reserve.utilization_bp,
        reserve.reserve_factor_bp,
    )
    .map_err(|msg| ReducerError::Invariant(format!("morpho_optimizer pool supply: {msg}")))?;
    let p2p_mid = midpoint_decimal(&pool_supply, &pool_borrow)?;
    // Blend toward pool_supply as utilization rises (more position sits in
    // pool fallback).
    let supply_match_bp = 10_000 - reserve.utilization_bp.min(10_000);
    blend_decimal(&p2p_mid, &pool_supply, supply_match_bp)
}

/// Compute the blended p2p borrow rate given the p2p index and the current
/// pool borrow rate fallback.
pub(super) fn p2p_borrow_rate(reserve: &ReserveState) -> ReducerResult<Decimal> {
    let pool_borrow = shared::two_slope_borrow_apr(
        reserve.utilization_bp,
        POOL_BASE_RATE_BP,
        POOL_SLOPE1_BP,
        POOL_SLOPE2_BP,
        POOL_OPTIMAL_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("morpho_optimizer pool rate: {msg}")))?;
    let pool_supply = shared::supply_apr_from_borrow(
        &pool_borrow,
        reserve.utilization_bp,
        reserve.reserve_factor_bp,
    )
    .map_err(|msg| ReducerError::Invariant(format!("morpho_optimizer pool supply: {msg}")))?;
    let p2p_mid = midpoint_decimal(&pool_supply, &pool_borrow)?;
    let borrow_match_bp = 10_000 - reserve.utilization_bp.min(10_000);
    blend_decimal(&p2p_mid, &pool_borrow, borrow_match_bp)
}

/// Convert an asset amount into the equivalent Optimizer share amount using
/// the current p2p/pool index split.
/// 1:1 mapping; the body validates the reserve and returns the underlying
/// asset amount. When the sync orchestrator wires real per-market p2p /
/// pool indices the body switches to the Optimizer's split form.
pub(super) fn asset_to_optimizer_shares(
    reserve: &ReserveState,
    asset_amount: U256,
) -> ReducerResult<U256> {
    shared::asset_to_scaled_balance(reserve, asset_amount, "morpho_optimizer")
}

/// `(a + b) / 2` as a `Decimal`. Used to compute the p2p midpoint.
fn midpoint_decimal(a: &Decimal, b: &Decimal) -> ReducerResult<Decimal> {
    use std::str::FromStr;
    let a = rust_decimal::Decimal::from_str(a.as_str())
        .map_err(|e| ReducerError::Invariant(format!("midpoint a parse: {e}")))?;
    let b = rust_decimal::Decimal::from_str(b.as_str())
        .map_err(|e| ReducerError::Invariant(format!("midpoint b parse: {e}")))?;
    let two = rust_decimal::Decimal::from(2u32);
    let mid = (a + b) / two;
    Ok(Decimal::new(mid.normalize().to_string()))
}

/// `weight_bp * a + (10_000 - weight_bp) * b`, all divided by `10_000`.
/// Linear blend between two `Decimal`s.
fn blend_decimal(a: &Decimal, b: &Decimal, weight_bp: u32) -> ReducerResult<Decimal> {
    use std::str::FromStr;
    if weight_bp > 10_000 {
        return Err(ReducerError::Invariant(format!(
            "blend weight {weight_bp} bp > 10000"
        )));
    }
    let a_dec = rust_decimal::Decimal::from_str(a.as_str())
        .map_err(|e| ReducerError::Invariant(format!("blend a parse: {e}")))?;
    let b_dec = rust_decimal::Decimal::from_str(b.as_str())
        .map_err(|e| ReducerError::Invariant(format!("blend b parse: {e}")))?;
    let w = rust_decimal::Decimal::new(i64::from(weight_bp), 4);
    let inv_w = rust_decimal::Decimal::new(i64::from(10_000 - weight_bp), 4);
    let blended = a_dec * w + b_dec * inv_w;
    Ok(Decimal::new(blended.normalize().to_string()))
}

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

    /// At `U = 50 %`:
    /// `pool_borrow = 0 + 400 * 5000/8000 = 250 bp = 0.025`.
    /// `pool_supply = 0.025 * 0.5 * 0.9 = 0.01125`.
    /// `p2p_mid = (0.025 + 0.01125) / 2 = 0.018125`.
    /// `match_ratio = 10000 - 5000 = 5000 bp = 50 %`.
    /// `p2p_supply = 0.018125 * 0.5 + 0.01125 * 0.5 = 0.0146875`.
    #[test]
    fn p2p_supply_rate_below_kink() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let s = p2p_supply_rate(&r).unwrap();
        assert_eq!(s.as_str(), "0.0146875");
    }

    /// `p2p_borrow` at the same `U = 50 %`:
    /// `p2p_borrow = 0.018125 * 0.5 + 0.025 * 0.5 = 0.0215625`.
    #[test]
    fn p2p_borrow_rate_below_kink() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let b = p2p_borrow_rate(&r).unwrap();
        assert_eq!(b.as_str(), "0.0215625");
    }

    /// At `U = 100 %` the borrow `match_ratio = 0` → full pool fallback.
    /// `pool_borrow = base + slope1 + slope2 = 0 + 0.04 + 0.60 = 0.64`.
    /// `p2p_borrow = pool_borrow = 0.64`.
    #[test]
    fn p2p_borrow_rate_max_util_falls_to_pool() {
        let r = reserve_with(10_000, 10_000, 10_000);
        let b = p2p_borrow_rate(&r).unwrap();
        assert_eq!(b.as_str(), "0.64");
    }

    /// `asset_to_optimizer_shares` round-trip under the index=1 approx.
    #[test]
    fn asset_to_optimizer_shares_one_to_one() {
        let r = reserve_with(10_000, 5_000, 5_000);
        let s = asset_to_optimizer_shares(&r, U256::from(2_500u64)).unwrap();
        assert_eq!(s, U256::from(2_500u64));
    }

    /// Out-of-range utilization rejected.
    #[test]
    fn p2p_supply_rate_oversaturated_errors() {
        let r = reserve_with(10_000, 9_000, 10_500);
        let err = p2p_supply_rate(&r).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
