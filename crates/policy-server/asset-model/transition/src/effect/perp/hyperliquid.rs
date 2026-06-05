//! Hyperliquid venue math — off-chain orderbook mark prices, funding rates,
//! position lifecycle.
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::Hyperliquid`. Not a `Reducer` impl.
//! ## Formulas
//! Hyperliquid follows the standard off-chain-orderbook margin model:
//! ```text
//!   notional             = size_base × mark_price
//!   required_margin      = notional / leverage + taker_fee_bp × notional / 10_000
//!   maintenance_margin   = notional × maintenance_bp / 10_000
//!   liquidation_price    = entry ± (free_margin − maintenance_margin) / size_base
//!                              (− for long, + for short)
//!   unrealized_pnl       = size_base × (mark − entry) × side_sign
//!   funding_accrued      = size_base × funding_rate × hours_elapsed / 24
//! ```
//! `liquidation_price` follows the simplified single-asset isolated-margin
//! form used by Hyperliquid's Python SDK (`hyperliquid-python-sdk` ::
//! `info.user_state` returns `liquidationPx` with the same algebraic shape).
//! Cross-margin accounts use the same formula with `free_margin` = total
//! account collateral; the `LiveField` `user_account_state.free_margin_usd`
//! captures whichever the venue reports.
//! ## Primary sources
//! - <https://hyperliquid.gitbook.io/hyperliquid-docs/trading/perpetual-assets>
//!   — margin / liquidation reference
//! - <https://github.com/hyperliquid-dex/hyperliquid-python-sdk> —
//!   `info.user_state` `liquidationPx` field shape

#![allow(clippy::module_name_repetitions)]
#![allow(dead_code)]

use policy_state::primitives::{Decimal, Price, SignedI256, U256};
use policy_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on Hyperliquid.
/// `required_margin = notional / leverage + taker_fee × notional / 10_000`.
/// `notional` is derived from `size_base × mark_price` where both inputs come
/// from `live.user_account_state` / `live.mark_price`. The taker fee uses
/// `live.fee_taker_bp` (Hyperliquid quotes maker/taker fees in bp).
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("hyperliquid", action, live)
}

/// Compute the liquidation price of a newly opened position on Hyperliquid.
/// Hyperliquid uses the simplified isolated-margin form:
/// ```text
///   liq_price = entry ± (free_margin − maintenance_margin) / size_base
/// ```
/// Long subtracts, short adds. `maintenance_margin = notional ×
/// maintenance_bp / 10_000`. Returns `Ok(None)` if `size_base = 0` (no
/// position to liquidate).
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_simple("hyperliquid", action, live)
}

/// Compute unrealized `PnL` given size, entry price, and current mark price.
/// Formula: `pnl = size_base × (mark − entry) × side_sign` where
/// `side_sign = +1` for long and `−1` for short. Truncates fractional units
/// toward zero (venues book at integer denom).
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at`.
/// Simplified formula: `funding = size_base × funding_rate × hours_elapsed
/// / 24`. Hyperliquid pays funding hourly; the divisor stays at `24` so the
/// `LiveField` `funding_rate` carries the venue's natural denomination (daily
/// rate). Positive result = funding paid *to* the position (long during
/// negative funding); negative = funding paid *from*.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PnL` long: 2 ETH × (3100 − 3000) = +200.
    #[test]
    fn pnl_long_profit_matches_helpers_derived_fixture() {
        let pnl = unrealized_pnl(
            U256::from(2_u64),
            &Decimal::new("3000"),
            &Decimal::new("3100"),
            true,
        )
        .unwrap();
        assert_eq!(pnl, SignedI256::try_from(200_i32).unwrap());
    }

    /// `PnL` short: 3 ETH × (3000 − 2800) = +600 for a short on a drop.
    #[test]
    fn pnl_short_profit_on_drop() {
        let pnl = unrealized_pnl(
            U256::from(3_u64),
            &Decimal::new("3000"),
            &Decimal::new("2800"),
            false,
        )
        .unwrap();
        assert_eq!(pnl, SignedI256::try_from(600_i32).unwrap());
    }

    /// Funding: 1 ETH × 0.01 (1% daily) × 24h / 24 = 0.01 → trunc → 0
    /// (sub-integer truncation; matches venue integer-denom booking).
    #[test]
    fn funding_accrued_trunc_to_zero_for_sub_integer() {
        let fund = funding_accrued(U256::from(1_u64), &Decimal::new("0.01"), 24).unwrap();
        assert_eq!(fund, SignedI256::ZERO);
    }

    /// Funding: 1000 ETH × 0.01 × 24h / 24 = 10.
    #[test]
    fn funding_accrued_integer_result() {
        let fund = funding_accrued(U256::from(1_000_u64), &Decimal::new("0.01"), 24).unwrap();
        assert_eq!(fund, SignedI256::try_from(10_i32).unwrap());
    }

    /// `unrealized_pnl` rejects unparseable mark price.
    #[test]
    fn unrealized_pnl_rejects_invalid_mark() {
        use crate::error::ReducerError;
        let err = unrealized_pnl(
            U256::from(1_u64),
            &Decimal::new("3000"),
            &Decimal::new("not-a-number"),
            true,
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
