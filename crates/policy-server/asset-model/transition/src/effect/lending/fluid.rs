//! Fluid venue math — smart debt / smart collateral.
//!
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::Fluid`. Not a `Reducer` impl.
//!
//! Fluid uses unified vault state combining supply and borrow.
//!
//! ## Vault shares
//!
//! [fToken.sol](https://github.com/Instadapp/fluid-protocol/) — Fluid's
//! ERC4626-style vault — exposes `convertToShares` / `convertToAssets`:
//!
//! ```text
//!   shares = (assets * (totalShares + 1)) / (totalAssets + 1)
//!   assets = (shares * (totalAssets + 1)) / (totalShares + 1)
//! ```
//!
//! The `+ 1` virtual offset is identical to `OpenZeppelin`'s ERC4626 mitigation
//! against share-price manipulation (Fluid uses `VIRTUAL_SHARES = VIRTUAL_ASSETS
//! = 1`, smaller than Morpho's `VIRTUAL_SHARES = 1e6`).
//!
//! ## Rate model
//!
//! Fluid uses Aave-style two-slope variable borrow rates with vault-specific
//! parameters. Until vault-specific strategy state is plumbed through, we use
//! Aave V3's defaults via the `shared` module.

// Phase 2 stubs: per-action wiring lands in a later commit in the same batch.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, U256};

// `ReserveState` is reused here as a stand-in until a Fluid-specific vault
// state struct is added in Phase 2.
use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

use super::shared;

/// Fluid `VIRTUAL_SHARES` / `VIRTUAL_ASSETS` offset (`1`).
const VIRTUAL_OFFSET: u128 = 1;

/// Convert an asset amount into the equivalent Fluid share amount using the
/// current vault exchange rate.
pub(super) fn asset_to_fluid_shares(
    vault_state: &ReserveState,
    asset_amount: U256,
) -> ReducerResult<U256> {
    let total_assets = vault_state.total_supply;
    // `total_shares` is approximated as `total_supply` under the Phase 2
    // index=1 model — Fluid's vault tracks share supply separately, but
    // until the orchestrator wires it through they are identical at the
    // start of every vault's life.
    let total_shares = vault_state.total_supply;

    let numer_factor = total_shares
        .checked_add(U256::from(VIRTUAL_OFFSET))
        .ok_or_else(|| ReducerError::Invariant("fluid: virtual shares overflow".into()))?;
    let denom = total_assets
        .checked_add(U256::from(VIRTUAL_OFFSET))
        .ok_or_else(|| ReducerError::Invariant("fluid: virtual assets overflow".into()))?;
    let numer = asset_amount
        .checked_mul(numer_factor)
        .ok_or_else(|| ReducerError::Invariant("fluid: asset_to_shares overflow".into()))?;
    if denom.is_zero() {
        return Err(ReducerError::Invariant(
            "fluid: zero denominator (impossible with virtual offset)".into(),
        ));
    }
    Ok(numer / denom)
}

/// Inverse of [`asset_to_fluid_shares`].
pub(super) fn fluid_shares_to_asset(
    vault_state: &ReserveState,
    share_amount: U256,
) -> ReducerResult<U256> {
    let total_assets = vault_state.total_supply;
    let total_shares = vault_state.total_supply;

    let numer_factor = total_assets
        .checked_add(U256::from(VIRTUAL_OFFSET))
        .ok_or_else(|| ReducerError::Invariant("fluid: virtual assets overflow".into()))?;
    let denom = total_shares
        .checked_add(U256::from(VIRTUAL_OFFSET))
        .ok_or_else(|| ReducerError::Invariant("fluid: virtual shares overflow".into()))?;
    let numer = share_amount
        .checked_mul(numer_factor)
        .ok_or_else(|| ReducerError::Invariant("fluid: shares_to_asset overflow".into()))?;
    if denom.is_zero() {
        return Err(ReducerError::Invariant(
            "fluid: zero denominator (impossible with virtual offset)".into(),
        ));
    }
    Ok(numer / denom)
}

/// Compute the per-year (APR) borrow rate on a vault given its current
/// utilization.
pub(super) fn current_borrow_rate(vault_state: &ReserveState) -> ReducerResult<Decimal> {
    shared::two_slope_borrow_apr(
        vault_state.utilization_bp,
        FLUID_BASE_RATE_BP,
        FLUID_SLOPE1_BP,
        FLUID_SLOPE2_BP,
        FLUID_OPTIMAL_BP,
    )
    .map_err(|msg| ReducerError::Invariant(format!("fluid rate: {msg}")))
}

/// Fluid default base rate (per year, basis points). Most vaults run at 0 %.
const FLUID_BASE_RATE_BP: u32 = 0;
/// Fluid default `slope1`.
const FLUID_SLOPE1_BP: u32 = 400;
/// Fluid default `slope2`.
const FLUID_SLOPE2_BP: u32 = 6_000;
/// Fluid default `kink` (`80 %`).
const FLUID_OPTIMAL_BP: u32 = 8_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with(total_supply: u128, total_borrow: u128, utilization_bp: u32) -> ReserveState {
        ReserveState {
            total_supply: U256::from(total_supply),
            total_borrow: U256::from(total_borrow),
            utilization_bp,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 8_500,
            liquidation_threshold_bp: 9_000,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1_000,
            is_frozen: false,
            is_paused: false,
        }
    }

    /// Empty vault: assets=0, shares=0. Supply 1000 assets → shares =
    /// `1000 * (0 + 1) / (0 + 1) = 1000` (virtual offset normalises).
    #[test]
    fn asset_to_shares_empty_vault() {
        let v = vault_with(0, 0, 0);
        let s = asset_to_fluid_shares(&v, U256::from(1_000u64)).unwrap();
        assert_eq!(s, U256::from(1_000u64));
    }

    /// Round-trip: shares → assets → shares matches for in-equilibrium vault.
    #[test]
    fn shares_assets_round_trip() {
        let v = vault_with(1_000_000, 500_000, 5_000);
        let in_amt = U256::from(123_456u64);
        let shares = asset_to_fluid_shares(&v, in_amt).unwrap();
        let back = fluid_shares_to_asset(&v, shares).unwrap();
        assert_eq!(back, in_amt);
    }

    /// At kink (`U = 80 %`) the rate equals `base + slope1 = 4 %`.
    #[test]
    fn borrow_rate_at_kink() {
        let v = vault_with(10_000, 8_000, 8_000);
        let rate = current_borrow_rate(&v).unwrap();
        assert_eq!(rate.as_str(), "0.04");
    }

    /// Out-of-range utilization rejected.
    #[test]
    fn borrow_rate_oversaturated_errors() {
        let v = vault_with(10_000, 9_000, 10_500);
        let err = current_borrow_rate(&v).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
