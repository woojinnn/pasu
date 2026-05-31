//! `RegisterOperatorAction` — register the caller as an EigenLayer operator.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::Address;

use super::RestakingVenue;

/// Register as an operator. Models DelegationManager
/// `registerAsOperator(address initDelegationApprover, uint32 allocationDelay, string metadataURI)`.
/// `delegation_approver` is who must co-sign stakers delegating to this operator
/// (zero = open); `allocation_delay` is the operator's allocation/slashing delay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RegisterOperatorAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// Initial delegation approver (zero address = no approver required).
    #[tsify(type = "string")]
    pub delegation_approver: Address,
    /// Operator allocation/slashing delay (blocks), `uint32`.
    pub allocation_delay: u32,
    /// Operator metadata URI.
    pub metadata_uri: String,
}
