//! `IncreasePerpAction` — add size to an existing perpetual position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::PositionId;
use policy_state::primitives::U256;
use policy_state::token::TokenRef;

use super::open::OpenPerpLiveInputs;
use super::{PerpVenue, SizeSpec};

/// Increase size of an existing perpetual position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct IncreasePerpAction {
    /// Perpetual venue hosting the position.
    pub venue: PerpVenue,
    /// Identifier of the position to increase (`PositionId`).
    pub position_id: PositionId,
    /// Additional size to add (`SizeSpec`).
    pub size: SizeSpec,
    /// Optional extra collateral token and amount to post.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "[TokenRef, string]")]
    pub add_collateral: Option<(TokenRef, U256)>,
    /// Maximum acceptable slippage in basis points.
    pub slippage_bp: u32,
    /// Same `OpenPerpLiveInputs` as for opening a position.
    pub live_inputs: OpenPerpLiveInputs,
}
