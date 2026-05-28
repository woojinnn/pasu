//! Aevo venue math — off-chain orderbook with on-chain settlement on Aevo L2
//! (Optimism stack).
//!
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::Aevo`. Not a `Reducer` impl.
//!
//! ## Formulas
//!
//! Identical algebraic form to Hyperliquid (off-chain orderbook isolated
//! margin); the only knob is the venue tag carried in error messages.
//!
//! ## Primary sources
//!
//! - <https://docs.aevo.xyz/portfolio-margin/margin-overview> — margin /
//!   liquidation reference
//! - <https://docs.aevo.xyz/reference/api> — `account` endpoint
//!   `liquidation_price` field shape

#![allow(dead_code)]

use simulation_state::primitives::{Decimal, Price, SignedI256, U256};
use simulation_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on Aevo.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("aevo", action, live)
}

/// Compute the liquidation price of a newly opened position on Aevo.
///
/// Aevo uses single-asset isolated margin — same closed form as Hyperliquid.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_simple("aevo", action, live)
}

/// Compute unrealized `PnL` on Aevo given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on Aevo.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
