//! `ClaimFundingAction` — claim accrued funding payments from one or all markets.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{MarketRef, U256};
use simulation_state::token::TokenRef;
use simulation_state::LiveField;

use super::PerpVenue;

/// Claim accrued funding payments from one or all markets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimFundingAction {
    /// Perpetual venue to claim funding from.
    pub venue: PerpVenue,
    /// None = all markets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub market: Option<MarketRef>,
    /// Live claimable-funding inputs.
    pub live_inputs: ClaimFundingLiveInputs,
}

/// Live inputs read at execution time for `ClaimFundingAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimFundingLiveInputs {
    /// Claimable funding amounts grouped by `TokenRef`.
    #[tsify(type = "LiveField<Array<[TokenRef, string]>>")]
    pub claimable: LiveField<Vec<(TokenRef, U256)>>,
}
