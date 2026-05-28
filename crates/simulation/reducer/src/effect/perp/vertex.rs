//! Vertex venue math — hybrid orderbook + AMM; supports spot, perp, and money
//! market in one venue.
//!
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::Vertex`. Not a `Reducer` impl.
//!
//! ## Sequencer-routed settlement
//!
//! Vertex routes order matching through an off-chain sequencer but settles
//! on-chain at the per-product clearinghouse contract. From the reducer's
//! perspective the position is on-chain (state mutates immediately upon
//! settlement), unlike Hyperliquid / Aevo where the orderbook is itself the
//! source of truth.
//!
//! ## Formulas
//!
//! Vertex publishes per-product `maintenance_margin_fraction` and
//! `initial_margin_fraction` in the clearinghouse `SubaccountUtils` library.
//! Our `LiveFields` surface them as `maintenance_bp` / `initial_margin_bp`;
//! the closed-form simple-margin formula applies for the dominant-position
//! case (matches the venue's `SubaccountUtils.healthCheck` linearisation).
//!
//! ## Primary sources
//!
//! - <https://docs.vertexprotocol.com/developer-resources/api/v2/intro> —
//!   `subaccount/info` endpoint for live margin params
//! - <https://github.com/vertex-protocol/vertex-contracts> —
//!   `SubaccountUtils.sol::healthCheck`

#![allow(dead_code)]

use simulation_state::primitives::{Decimal, Price, SignedI256, U256};
use simulation_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on Vertex.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("vertex", action, live)
}

/// Compute the liquidation price of a newly opened position on Vertex.
///
/// Vertex's `SubaccountUtils.healthCheck` linearises the multi-product
/// healthbook around the dominant position; the simple-margin closed form
/// is accurate in that regime.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_simple("vertex", action, live)
}

/// Compute unrealized `PnL` on Vertex given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on Vertex.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
