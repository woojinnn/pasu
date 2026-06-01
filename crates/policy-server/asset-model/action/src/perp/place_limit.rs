//! `PlaceLimitOrderAction` — place a limit order on the venue's orderbook.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::PerpSide;
use policy_state::primitives::{MarketRef, Price};
use policy_state::LiveField;

use super::{PerpAccountState, PerpVenue, SizeSpec, TimeInForce};

/// Place a limit order on the venue's orderbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlaceLimitOrderAction {
    /// Perpetual venue receiving the order.
    pub venue: PerpVenue,
    /// Market symbol the order is placed on.
    pub market: MarketRef,
    /// Long or short (`PerpSide`).
    pub side: PerpSide,
    /// Order size (`SizeSpec`).
    pub size: SizeSpec,
    /// Limit `Price`.
    pub price: Price,
    /// Time-in-force policy (`TimeInForce`).
    pub time_in_force: TimeInForce,
    /// If `true`, the order may only reduce existing exposure.
    pub reduce_only: bool,
    /// Live market / account inputs.
    pub live_inputs: PlaceLimitLiveInputs,
}

/// Live inputs read at execution time for `PlaceLimitOrderAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlaceLimitLiveInputs {
    /// Current mark `Price` for the market.
    pub mark_price: LiveField<Price>,
    /// Best bid / ask `Price` pair used for spread validation.
    pub best_bid_ask: LiveField<(Price, Price)>,
    /// Number of open orders — used to check venue per-user limits.
    pub open_orders_count: LiveField<u32>,
    /// Current `PerpAccountState` for the user.
    pub user_account_state: LiveField<PerpAccountState>,
}
