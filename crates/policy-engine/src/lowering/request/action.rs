//! Leaf `Action` to action-specific `PolicyRequest` conversion.

use crate::core::Action;
use crate::policy::PolicyRequest;

/// Build a `PolicyRequest` from a fully-enriched `Action`. This is the public
/// "Action -> Cedar request" conversion used by `Pipeline` lowering.
#[must_use]
pub fn request_from_action(action: &Action) -> PolicyRequest {
    match action {
        Action::Dex(d) => super::dex::request(d),
        Action::Other(o) => super::other::request(o),
    }
}

/// Build one `PolicyRequest` from an action. The aggregate Dex action already
/// represents the full transaction-level intent.
#[must_use]
pub fn requests_from_action(action: &Action) -> Vec<PolicyRequest> {
    vec![request_from_action(action)]
}

/// Build one policy request for each action in input order.
#[must_use]
pub fn requests_from_actions(actions: &[Action]) -> Vec<PolicyRequest> {
    actions.iter().map(request_from_action).collect()
}
