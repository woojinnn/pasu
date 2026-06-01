//! Morpho Blue venue math — immutable per-market lending primitive.
//! Pure functions called from per-action reducers (`supply.rs`, `borrow.rs`, ...)
//! after dispatch on `LendingVenue::MorphoBlue`. Not a `Reducer` impl.
//! Per-market state; `market_id = keccak((loan, collat, oracle, irm, lltv))`.
//! The interest-rate model (IRM) is a separate contract whose state lives
//! outside the market itself.
//! ## Shares math
//! Morpho Blue uses [OpenZeppelin's ERC4626 "virtual offset" pattern](https://github.com/morpho-org/morpho-blue/blob/main/src/libraries/SharesMathLib.sol)
//! to prevent share-price manipulation:
//! ```text
//!   VIRTUAL_SHARES = 1e6
//!   VIRTUAL_ASSETS = 1
//!   shares_to_assets_down(s)  = s * (totalAssets + VIRTUAL_ASSETS)
//!                              / (totalShares + VIRTUAL_SHARES)
//!   assets_to_shares_down(a)  = a * (totalShares + VIRTUAL_SHARES)
//!                              / (totalAssets + VIRTUAL_ASSETS)
//! ```
//! We implement the `Down` variants (favours the protocol on supply / withdraw
//! computations). The `Up` variants used for accounting-side defensiveness
//! can be added once a caller needs them.

#![allow(dead_code)]

use policy_state::primitives::{Decimal, U256};

// `ReserveState` is reused here as a stand-in until a Morpho-Blue-specific
use crate::action::lending::ReserveState;
use crate::error::{ReducerError, ReducerResult};

/// Morpho Blue `VIRTUAL_SHARES` offset (`1e6`).
const VIRTUAL_SHARES: u128 = 1_000_000;

/// Morpho Blue `VIRTUAL_ASSETS` offset (`1`).
const VIRTUAL_ASSETS: u128 = 1;

/// Convert a share balance into the equivalent asset amount using the market's
/// current total assets and total shares (rounded down).
pub(super) fn shares_to_assets(
    market_total_assets: U256,
    market_total_shares: U256,
    shares: U256,
) -> ReducerResult<U256> {
    let numer_factor = market_total_assets
        .checked_add(U256::from(VIRTUAL_ASSETS))
        .ok_or_else(|| ReducerError::Invariant("morpho_blue: virtual assets overflow".into()))?;
    let denom = market_total_shares
        .checked_add(U256::from(VIRTUAL_SHARES))
        .ok_or_else(|| ReducerError::Invariant("morpho_blue: virtual shares overflow".into()))?;
    if denom.is_zero() {
        return Err(ReducerError::Invariant(
            "morpho_blue: zero denominator (impossible — virtual shares are 1e6)".into(),
        ));
    }
    let numer = shares.checked_mul(numer_factor).ok_or_else(|| {
        ReducerError::Invariant("morpho_blue: shares_to_assets multiplication overflow".into())
    })?;
    Ok(numer / denom)
}

/// Inverse of [`shares_to_assets`] — `assets_to_shares` rounded down.
pub(super) fn assets_to_shares(
    market_total_assets: U256,
    market_total_shares: U256,
    assets: U256,
) -> ReducerResult<U256> {
    let numer_factor = market_total_shares
        .checked_add(U256::from(VIRTUAL_SHARES))
        .ok_or_else(|| ReducerError::Invariant("morpho_blue: virtual shares overflow".into()))?;
    let denom = market_total_assets
        .checked_add(U256::from(VIRTUAL_ASSETS))
        .ok_or_else(|| ReducerError::Invariant("morpho_blue: virtual assets overflow".into()))?;
    if denom.is_zero() {
        return Err(ReducerError::Invariant(
            "morpho_blue: zero denominator (impossible — virtual assets are 1)".into(),
        ));
    }
    let numer = assets.checked_mul(numer_factor).ok_or_else(|| {
        ReducerError::Invariant("morpho_blue: assets_to_shares multiplication overflow".into())
    })?;
    Ok(numer / denom)
}

/// Query the market's IRM contract for the current per-year (APR) borrow rate.
/// **Deferred** — Morpho Blue's IRM (e.g. the Adaptive Curve IRM) is a
/// separate contract whose state is not part of the on-chain market struct.
/// The sync orchestrator must fetch the IRM state into a dedicated
/// `LiveField` before this function can return a meaningful rate. Until the
/// `LiveField` shape lands, the body surfaces `UnsupportedProtocol` so callers
/// fail fast.
pub(super) fn current_borrow_rate(_irm_state: &ReserveState) -> ReducerResult<Decimal> {
    Err(ReducerError::UnsupportedProtocol {
        action: "current_borrow_rate".into(),
        protocol: "morpho_blue".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty market: `assets = 0, shares = 0`. Supplying `1_000_000` assets
    /// yields `1_000_000 * (0 + 1_000_000) / (0 + 1) = 1e12` shares (the
    /// 1e6 virtual share offset bootstraps the first depositor's share
    /// account at 1e6 shares per 1 wei of asset).
    #[test]
    fn assets_to_shares_empty_market_uses_virtual_offset() {
        let shares = assets_to_shares(U256::ZERO, U256::ZERO, U256::from(1_000_000u64)).unwrap();
        // 1_000_000 * (0 + 1e6) / (0 + 1) = 1e12
        assert_eq!(shares, U256::from(1_000_000_000_000_u128));
    }

    /// Round-trip: `shares_to_assets(assets_to_shares(a)) == a` on an
    /// in-equilibrium market (rounding down means equality holds when the
    /// numerator is divisible by the denominator).
    #[test]
    fn assets_shares_round_trip_in_equilibrium() {
        let total_assets = U256::from(1_000_000_u128);
        let total_shares = U256::from(1_000_000_000_000_u128);

        let in_amt = U256::from(500_u64);
        let shares = assets_to_shares(total_assets, total_shares, in_amt).unwrap();
        let back = shares_to_assets(total_assets, total_shares, shares).unwrap();
        // After one round trip we expect equality within 1 wei (rounding
        // down on both sides). For these numbers the division is exact.
        assert_eq!(back, in_amt);
    }

    /// Existing market — supply price grows with `total_assets`.
    /// Supplying `100` assets to a market with `(1_000 assets, 1e9 shares)`:
    /// shares = `100 * (1e9 + 1e6) / (1_000 + 1) = 100 * 1_001_000_000 / 1001`
    ///        = `100_000_000` (round down).
    #[test]
    fn assets_to_shares_existing_market() {
        let total_assets = U256::from(1_000_u64);
        let total_shares = U256::from(1_000_000_000_u64);
        let shares = assets_to_shares(total_assets, total_shares, U256::from(100u64)).unwrap();
        // 100 * 1_001_000_000 = 100_100_000_000; / 1001 = 99_999_x… let's check:
        // 100_100_000_000 / 1001 = 99_999_x exactly? 1001 * 100_000_000 = 100_100_000_000, so 100_000_000.
        assert_eq!(shares, U256::from(100_000_000u64));
    }

    /// `current_borrow_rate` deferred — surfaces `UnsupportedProtocol` so
    /// the sync orchestrator wiring is the single point of activation.
    #[test]
    fn current_borrow_rate_is_deferred() {
        let dummy = ReserveState {
            total_supply: U256::from(1_000u64),
            total_borrow: U256::from(500u64),
            utilization_bp: 5_000,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 8_600,
            liquidation_threshold_bp: 8_600,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1_000,
            is_frozen: false,
            is_paused: false,
        };
        let err = current_borrow_rate(&dummy).unwrap_err();
        assert!(
            matches!(err, ReducerError::UnsupportedProtocol { ref protocol, .. } if protocol == "morpho_blue")
        );
    }
}
