//! `DelegateToAction` — delegate all deposited restaking shares to an operator.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;

use super::RestakingVenue;

/// Delegate to an operator.
///
/// Models `DelegationManager`
/// `delegateTo(address operator, SignatureWithExpiry approverSignatureAndExpiry, bytes32 approverSalt)`.
/// `operator` receives delegated validation power over ALL of the staker's
/// restaked shares; `approver_salt` de-collides the operator-approver EIP-712
/// signature (the approver's grant is captured off-chain — `DelegationApproval`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DelegateToAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// Operator receiving the delegation.
    #[tsify(type = "string")]
    pub operator: Address,
    /// `bytes32` approver-signature de-collision salt (hex).
    pub approver_salt: String,
}
