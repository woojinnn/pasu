//! Governance lifecycle helper actions.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::GovernanceVenue;

/// Redeem cancellation fees for a governance proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceRedeemCancellationFeeAction {
    /// Governance venue.
    pub venue: GovernanceVenue,
    /// Proposal id.
    #[tsify(type = "string")]
    pub proposal_id: U256,
}

/// Update an Aave Governance V3 representative for a specific chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceUpdateRepresentativeAction {
    /// Governance venue.
    pub venue: GovernanceVenue,
    /// Representative address being registered.
    #[tsify(type = "string")]
    pub representative: Address,
    /// Chain id represented by this address.
    #[tsify(type = "string")]
    pub representative_chain_id: U256,
}
