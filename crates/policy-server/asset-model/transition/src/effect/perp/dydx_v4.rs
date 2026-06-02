//! dYdX V4 venue math — dYdX V4 runs on its own Cosmos chain; off-chain
//! orderbook with on-chain settlement.
//! Pure functions called from per-action reducers (`open.rs`, `close.rs`, ...)
//! after dispatch on `PerpVenue::DyDxV4`. Not a `Reducer` impl.
//! ## Cross-margin only
//! dYdX V4 enforces cross-margin at the subaccount level. The simplified
//! liquidation-price formula degenerates to the same single-asset form when
//! exactly one position dominates the subaccount; for true multi-position
//! cross-margin the venue's `clob` module surfaces the canonical figure via
//! the indexer API and our `LiveField` source should switch to
//! `DataSource::VenueApi { provider: "dydx_v4_indexer", ... }`.
//! ## Primary sources
//! - <https://github.com/dydxprotocol/v4-chain/tree/main/protocol/x/clob>
//!   — `clob` module margin / liquidation reference (Cosmos SDK)
//! - <https://docs.dydx.exchange/api_integration-indexer/indexer_api> —
//!   indexer API `liquidationPx` field shape
//! ## Cosmos signing note
//! `DyDx` V4 trade orders are signed via Cosmos SDK `SIGN_MODE_DIRECT`, not
//! EIP-712. The orderbook-vs-on-chain dispatch in the per-action reducers
//! treats `DyDx` V4 the same as Hyperliquid (pending-only state mutation at
//! signing); the actual signature payload format differs, but our
//! `PendingTx.signature_payload: Vec<u8>` is signature-scheme-agnostic.

#![allow(dead_code)]

use policy_state::primitives::{Decimal, Price, SignedI256, U256};
use policy_state::{EvalContext, WalletState};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs};
use crate::error::ReducerResult;

use super::math;

/// Compute the initial margin required for an `OpenPerpAction` on dYdX V4.
pub(super) fn required_initial_margin(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    math::required_initial_margin_common("dydx_v4", action, live)
}

/// Compute the liquidation price of a newly opened position on dYdX V4.
/// dYdX V4 is cross-margin only; the simplified single-asset formula remains
/// correct for the common case (single dominant position). Multi-position
/// cross-margin should be sourced from the venue indexer's `liquidationPx`.
pub(super) fn liquidation_price(
    _state: &WalletState,
    _ctx: &EvalContext,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    math::liquidation_price_simple("dydx_v4", action, live)
}

/// Compute unrealized `PnL` on dYdX V4 given size, entry price, and current
/// mark price.
pub(super) fn unrealized_pnl(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    math::unrealized_pnl_common(size_base, entry, mark, is_long)
}

/// Compute funding accrued on a position since `last_funding_at` on dYdX V4.
pub(super) fn funding_accrued(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    math::funding_accrued_common(size_base, funding_rate, hours_elapsed)
}
