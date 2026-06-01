//! `ClosePerpAction` — close (fully or partially) an existing perpetual position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::position::PositionId;
use simulation_state::primitives::{Price, SignedI256};
use simulation_state::LiveField;

use super::{PerpVenue, SizeSpec};

/// Close (fully or partially) an existing perpetual position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClosePerpAction {
    /// Perpetual venue hosting the position.
    pub venue: PerpVenue,
    /// Identifier of the position to close (`PositionId`).
    pub position_id: PositionId,
    /// None = full close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub size: Option<SizeSpec>,
    /// Maximum acceptable slippage in basis points.
    pub slippage_bp: u32,
    /// Live market / position inputs.
    pub live_inputs: ClosePerpLiveInputs,
}

/// Live inputs read at execution time for `ClosePerpAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClosePerpLiveInputs {
    /// Current mark `Price` for the market.
    pub mark_price: LiveField<Price>,
    /// Unrealized `PnL` on the position at execution time.
    #[tsify(type = "LiveField<string>")]
    pub unrealized_pnl_now: LiveField<SignedI256>,
    /// Funding accrued on the position so far.
    #[tsify(type = "LiveField<string>")]
    pub funding_accrued: LiveField<SignedI256>,
    /// Fee in basis points to apply on close.
    pub fee_bp: LiveField<u32>,
}
