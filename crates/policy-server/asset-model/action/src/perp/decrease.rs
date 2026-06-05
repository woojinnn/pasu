//! `DecreasePerpAction` — reduce size of an existing perpetual position without fully closing it.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::PositionId;

use super::close::ClosePerpLiveInputs;
use super::{PerpVenue, SizeSpec};

/// Decrease size of an existing perpetual position without closing it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DecreasePerpAction {
    /// Perpetual venue hosting the position.
    pub venue: PerpVenue,
    /// Identifier of the position to decrease (`PositionId`).
    pub position_id: PositionId,
    /// Size to remove (`SizeSpec`).
    pub size: SizeSpec,
    /// Maximum acceptable slippage in basis points.
    pub slippage_bp: u32,
    /// Live market / position inputs (shared with close).
    pub live_inputs: ClosePerpLiveInputs,
}
