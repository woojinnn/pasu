//! Jupiter Perps venue math — Jupiter Perps on Solana; uses JLP pool as
//! counterparty (similar to GMX V1 model).
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::JupiterPerps`. Not a `Reducer` impl.
//! ## Deferred liquidation price
//! Jupiter Perps uses a JLP-pool-counterparty model (GMX V1 lineage): the
//! liquidation price depends on the position's accrued borrow fees, JLP
//! pool utilisation, and the venue's `liquidationFeeUsd` parameter. The
//! borrow-fee accumulator state lives only in Jupiter Perps' on-chain
//! `Position` account; we cannot reconstruct it from the
//! `OpenPerpLiveInputs` `LiveFields` available at reducer time.
//! `liquidation_price` therefore returns `UnsupportedProtocol { …
//! "deferred — see venue API" }`; the canonical figure is sourced via the
//! Jupiter Perps REST endpoint (`positions/<address>`) and refreshed by
//! the sync orchestrator.
//! ## Primary sources
//! - <https://station.jup.ag/docs/perpetual-exchange/onchain-account-types>
//!   — `Position` account layout (Anchor)
//! - <https://station.jup.ag/docs/perpetual-exchange/fees> — fee /
//!   liquidation model description

#![allow(dead_code)]

use policy_state::primitives::{Decimal, Price, SignedI256, U256};
use policy_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on Jupiter
/// Perps.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("jupiter_perps", action, live)
}

/// Compute the liquidation price of a newly opened position on Jupiter Perps.
/// Returns `UnsupportedProtocol` — deferred, see module docs.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    _live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_deferred("jupiter_perps", &action.venue)
}

/// Compute unrealized `PnL` on Jupiter Perps given size, entry price, and
/// current mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on Jupiter
/// Perps.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
