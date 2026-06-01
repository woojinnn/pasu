//! `AdjustMarginAction` — add or withdraw collateral from an existing position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::PositionId;
use policy_state::primitives::{SignedI256, U256};
use policy_state::LiveField;

use super::{PerpPositionLive, PerpVenue};

/// Add or withdraw collateral from an existing position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AdjustMarginAction {
    /// Perpetual venue hosting the position.
    pub venue: PerpVenue,
    /// Identifier of the position being adjusted (`PositionId`).
    pub position_id: PositionId,
    /// Positive = deposit, negative = withdraw.
    #[tsify(type = "string")]
    pub delta: SignedI256,
    /// Live position / margin inputs.
    pub live_inputs: AdjustMarginLiveInputs,
}

/// Live inputs read at execution time for `AdjustMarginAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AdjustMarginLiveInputs {
    /// Current `PerpPositionLive` state.
    pub position_state: LiveField<PerpPositionLive>,
    /// Free margin remaining after the adjustment is applied.
    #[tsify(type = "LiveField<string>")]
    pub free_margin_after: LiveField<U256>,
}
