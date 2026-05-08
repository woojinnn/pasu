//! Leaf `Action` to action-specific `PolicyRequest` conversion.

use crate::core::Action;
use crate::host::HostCapabilities;
use crate::policy::{PolicyError, PolicyRequest};

/// Build a `PolicyRequest` from a fully-enriched `Action`. This is the public
/// "Action -> Cedar request" conversion used by `Pipeline` lowering.
///
/// # Errors
///
/// Returns an error for signature actions because those require host clock
/// stamping. Use [`request_from_action_with_host`] for those variants.
pub fn request_from_action(action: &Action) -> Result<PolicyRequest, PolicyError> {
    match action {
        Action::Dex(d) => Ok(super::dex::request(d)),
        Action::Other(o) => Ok(super::other::request(o)),
        Action::Permit2(_) | Action::Eip2612(_) | Action::Eip712Other(_) => Err(
            PolicyError::Request("signature actions require host clock lowering".into()),
        ),
    }
}

/// Build a `PolicyRequest` from a fully-enriched `Action`, using host
/// capabilities where lowering needs them.
///
/// # Errors
///
/// Returns an error when lowering cannot build the Cedar request.
pub fn request_from_action_with_host(
    action: &Action,
    host: &HostCapabilities<'_>,
) -> Result<PolicyRequest, PolicyError> {
    match action {
        Action::Dex(d) => Ok(super::dex::request(d)),
        Action::Other(o) => Ok(super::other::request(o)),
        Action::Permit2(p) => super::signature::permit2_request(p, host.clock().now()),
        Action::Eip2612(p) => super::signature::eip2612_request(p, host.clock().now()),
        Action::Eip712Other(o) => super::signature::eip712_other_request(o, host.clock().now()),
    }
}

/// Build one `PolicyRequest` from an action. The aggregate Dex action already
/// represents the full transaction-level intent.
///
/// # Errors
///
/// Returns an error when the action cannot be lowered without host
/// capabilities.
pub fn requests_from_action(action: &Action) -> Result<Vec<PolicyRequest>, PolicyError> {
    Ok(vec![request_from_action(action)?])
}

/// Build one policy request for each action in input order.
///
/// # Errors
///
/// Returns an error when any action cannot be lowered without host
/// capabilities.
pub fn requests_from_actions(actions: &[Action]) -> Result<Vec<PolicyRequest>, PolicyError> {
    actions.iter().map(request_from_action).collect()
}
