//! `UndelegateAction` — undelegate from the current operator.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;

use super::RestakingVenue;

/// Undelegate.
///
/// Models `DelegationManager` `undelegate(address staker)`: revokes
/// the delegation and queues a withdrawal of all delegated shares. Callable by
/// the staker, the operator, or the operator's delegationApprover.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct UndelegateAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// Staker being undelegated.
    #[tsify(type = "string")]
    pub staker: Address,
}
