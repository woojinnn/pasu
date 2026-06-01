//! GMX V2 venue math — on-chain perp DEX backed by `GM` market pools; mark
//! price from Pyth/Chainlink, funding paid to/from the `GM` pool.
//!
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::GmxV2`. Not a `Reducer` impl.
//! ## Deferred liquidation price
//! GMX V2's `PositionUtils.getLiquidationPrice` factors in:
//!   - the position's accumulated funding fees,
//!   - the borrowing fee state of the `GM` market,
//!   - the price-impact-on-close discount,
//!   - the cumulative pnl contribution to the pool reserve.
//!
//! The first two require funding-fee and borrowing-fee accumulator state
//! that lives only in `MarketUtils.sol`'s storage — we cannot reconstruct
//! it from the `OpenPerpLiveInputs` `LiveFields` available at reducer time
//! without recomputing the venue's entire fee book. `liquidation_price`
//! therefore returns `UnsupportedProtocol { … "deferred — see venue
//! subgraph" }`; the canonical figure is sourced via the GMX subgraph as
//! a separate `LiveField` on the resulting `PerpPosition` and refreshed by
//! the sync orchestrator.
//! ## Primary sources
//! - <https://github.com/gmx-io/gmx-synthetics/blob/main/contracts/position/PositionUtils.sol>
//!   — `getLiquidationPrice` reference
//! - <https://gmx-docs.io/docs/api/subgraph-queries> — `MarketsInfo` /
//!   `Positions` graph queries for the venue-canonical liquidation price

#![allow(dead_code)]

use policy_state::primitives::{Decimal, Price, SignedI256, U256};
use policy_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on GMX V2.
/// GMX V2 applies the same `notional / leverage + taker_fee × notional`
/// formula as the orderbook venues; the venue-specific "execution fee"
/// (Pyth oracle keeper payment) is charged in native ETH and lives outside
/// the perp margin model — caller-side `ActionMeta.gas_fee` will track it.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("gmx_v2", action, live)
}

/// Compute the liquidation price of a newly opened position on GMX V2.
/// Returns `UnsupportedProtocol` — deferred, see module docs.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    _live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_deferred("gmx_v2", &action.venue)
}

/// Compute unrealized `PnL` on GMX V2 given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on GMX V2.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
