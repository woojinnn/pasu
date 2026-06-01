//! Synthetix venue math — Synthetix Perps V3; multi-collateral, atomic
//! settlement against the SNX debt pool.
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::Synthetix`. Not a `Reducer` impl.
//! ## Deferred liquidation price
//! Synthetix V3 supports multi-collateral cross-margin: a position is
//! collateralized by the user's `MarginConfiguration` (which can carry
//! sUSD, USDC, ETH, SNX etc. with per-asset discount factors). The
//! `LiquidationModule.liquidate` flow uses the account's combined-asset
//! `getCollateralValue` mark-to-market under each collateral's discount,
//! plus the perp's `getFundingPayments` accumulator. We cannot soundly
//! linearise this to a single liquidation price from
//! `OpenPerpLiveInputs` alone. `liquidation_price` therefore returns
//! `UnsupportedProtocol { … "deferred — see venue subgraph" }`; the
//! canonical figure (per-position) is sourced via the Synthetix subgraph
//! and refreshed by the sync orchestrator.
//! ## Primary sources
//! - <https://github.com/Synthetixio/synthetix-v3/tree/main/markets/perps-market>
//!   — `LiquidationModule.sol`, `AsyncOrderModule.sol`, fee model
//! - <https://docs.synthetix.io/v/v3/for-developers/perps-protocol-overview>
//!   — high-level margin model documentation

#![allow(dead_code)]

use policy_state::primitives::{Decimal, Price, SignedI256, U256};
use policy_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on Synthetix.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("synthetix", action, live)
}

/// Compute the liquidation price of a newly opened position on Synthetix.
/// Returns `UnsupportedProtocol` — deferred, see module docs.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    _live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_deferred("synthetix", &action.venue)
}

/// Compute unrealized `PnL` on Synthetix given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on Synthetix.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
