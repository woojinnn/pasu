//! `GovernanceVoteAction` — vote on a DAO proposal.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::U256;

use super::GovernanceVenue;

/// Vote on a governance proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceVoteAction {
    /// Governance venue.
    pub venue: GovernanceVenue,
    /// Proposal id.
    #[tsify(type = "string")]
    pub proposal_id: U256,
    /// Support value. Aave V3 uses a boolean support flag.
    pub support: bool,
    /// Optional human-readable vote reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub reason: Option<String>,
}
