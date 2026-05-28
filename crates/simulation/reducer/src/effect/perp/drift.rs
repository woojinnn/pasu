//! Drift venue math — Drift runs on Solana; DLOB (decentralized limit
//! orderbook) plus AMM fallback.
//!
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::Drift`. Not a `Reducer` impl.
//!
//! ## On-chain settlement
//!
//! Drift V2 routes matches through both an off-chain DLOB and an on-chain
//! vAMM; settlement always lands on-chain in the user's `PerpPosition`
//! account. The reducer treats Drift as on-chain (immediate state update).
//!
//! ## Formulas
//!
//! Drift V2's `calculate_perp_liability_value` / `calculate_margin_requirement`
//! produce a linearised health check identical in shape to Vertex /
//! Hyperliquid; the simple-margin closed form applies.
//!
//! ## Primary sources
//!
//! - <https://github.com/drift-labs/protocol-v2> — `programs/drift/src/math`
//!   for margin / liquidation reference (Anchor / Solana)
//! - <https://docs.drift.trade/> — high-level margin documentation

#![allow(dead_code)]

use simulation_state::primitives::{Decimal, Price, SignedI256, U256};
use simulation_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on Drift.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("drift", action, live)
}

/// Compute the liquidation price of a newly opened position on Drift.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_simple("drift", action, live)
}

/// Compute unrealized `PnL` on Drift given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on Drift.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
