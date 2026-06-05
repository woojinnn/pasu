//! `RedelegateAction` — atomically undelegate and delegate to a new operator.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;

use super::RestakingVenue;

/// Redelegate to a new operator (ELIP-003).
///
/// Models `DelegationManager`
/// `redelegate(address newOperator, SignatureWithExpiry newOperatorApproverSig, bytes32 approverSalt)`:
/// undelegates from the current operator and delegates to `new_operator` in one
/// call — the same total-share delegation risk as `delegateTo`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RedelegateAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// New operator receiving the delegation.
    #[tsify(type = "string")]
    pub new_operator: Address,
    /// `bytes32` approver-signature de-collision salt (hex).
    pub approver_salt: String,
}
