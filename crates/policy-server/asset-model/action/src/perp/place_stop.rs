//! `PlaceStopOrderAction` — place a stop / take-profit order.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::position::PerpSide;
use simulation_state::primitives::{MarketRef, Price};
use simulation_state::LiveField;

use super::{PerpAccountState, PerpVenue, SizeSpec, StopOrderKind};

/// Place a stop / take-profit order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlaceStopOrderAction {
    /// Perpetual venue receiving the order.
    pub venue: PerpVenue,
    /// Market symbol the order is placed on.
    pub market: MarketRef,
    /// Long or short (`PerpSide`).
    pub side: PerpSide,
    /// Order size (`SizeSpec`).
    pub size: SizeSpec,
    /// Trigger `Price` at which the stop fires.
    pub trigger_price: Price,
    /// Kind of stop order (`StopOrderKind`).
    pub order_kind: StopOrderKind,
    /// Required only for `StopLimit` / `TakeProfitLimit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub limit_price: Option<Price>,
    /// If `true`, the order may only reduce existing exposure.
    pub reduce_only: bool,
    /// Live market / account inputs.
    pub live_inputs: PlaceStopLiveInputs,
}

/// Live inputs read at execution time for `PlaceStopOrderAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlaceStopLiveInputs {
    /// Current mark `Price` for the market.
    pub mark_price: LiveField<Price>,
    /// Current `PerpAccountState` for the user.
    pub user_account_state: LiveField<PerpAccountState>,
}
