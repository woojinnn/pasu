//! GMX V2 venue math — on-chain perp DEX backed by `GM` market pools; mark
//! price from Pyth/Chainlink, funding paid to/from the `GM` pool.
//!
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::GmxV2`. Not a `Reducer` impl.

// Phase 2 stubs: callers (per-action reducers) are still `todo!()` so these
// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use simulation_state::primitives::{Decimal, Price, SignedI256, U256};
use simulation_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

/// Compute the initial margin required for an `OpenPerpAction` on GMX V2.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    _action: &OpenPerpAction,
    _live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    todo!()
}

/// Compute the liquidation price of a newly opened position on GMX V2.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    _action: &OpenPerpAction,
    _live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    todo!()
}

/// Compute unrealized `PnL` on GMX V2 given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    _size_base: U256,
    _entry: Price,
    _mark: Price,
    _is_long: bool,
) -> ReducerResult<SignedI256> {
    todo!()
}

/// Compute funding accrued on a position since `last_funding_at` on GMX V2.
pub(super) fn funding_accrued(
    _size_base: U256,
    _funding_rate: Decimal,
    _hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    todo!()
}
