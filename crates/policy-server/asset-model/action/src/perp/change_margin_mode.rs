//! `ChangeMarginModeAction` — switch margin mode (`Cross` <-> `Isolated`) for a market.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::{MarginMode, PositionId};
use policy_state::primitives::{MarketRef, U256};
use policy_state::LiveField;

use super::PerpVenue;

/// Switch margin mode (`Cross` <-> `Isolated`) for a market.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ChangeMarginModeAction {
    /// Perpetual venue on which margin mode is being changed.
    pub venue: PerpVenue,
    /// Market the new mode applies to.
    pub market: MarketRef,
    /// New `MarginMode` (cross or isolated).
    pub new_mode: MarginMode,
    /// Live venue / position inputs.
    pub live_inputs: ChangeMarginModeLiveInputs,
}

/// Live inputs read at execution time for `ChangeMarginModeAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ChangeMarginModeLiveInputs {
    /// Positions affected by the margin-mode switch.
    pub affected_positions: LiveField<Vec<PositionId>>,
    /// Resulting margin reallocation per affected position.
    #[tsify(type = "LiveField<Array<[PositionId, string]>>")]
    pub margin_reallocation: LiveField<Vec<(PositionId, U256)>>,
}
