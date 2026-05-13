//! Leaf `LegacyAction` to action-specific `PolicyRequest` conversion.

use crate::core::LegacyAction;
use crate::host::HostCapabilities;
use crate::policy::{PolicyError, PolicyRequest};

/// Build a `PolicyRequest` from a fully-enriched `LegacyAction`. This is the
/// public "`LegacyAction` -> Cedar request" conversion used by `Pipeline`
/// lowering.
///
/// # Errors
///
/// Returns an error for signature actions because those require host clock
/// stamping. Use [`request_from_action_with_host`] for those variants.
pub fn request_from_action(action: &LegacyAction) -> Result<PolicyRequest, PolicyError> {
    match action {
        LegacyAction::Dex(d) => Ok(super::dex::request(d)),
        LegacyAction::Other(o) => Ok(super::other::request(o)),
        LegacyAction::Permit2(_) | LegacyAction::Eip2612(_) | LegacyAction::Eip712Other(_) => Err(
            PolicyError::Request("signature actions require host clock lowering".into()),
        ),
    }
}

/// Build a `PolicyRequest` from a fully-enriched `LegacyAction`, using host
/// capabilities where lowering needs them.
///
/// # Errors
///
/// Returns an error when lowering cannot build the Cedar request.
pub fn request_from_action_with_host(
    action: &LegacyAction,
    host: &HostCapabilities<'_>,
) -> Result<PolicyRequest, PolicyError> {
    match action {
        LegacyAction::Dex(d) => Ok(super::dex::request(d)),
        LegacyAction::Other(o) => Ok(super::other::request(o)),
        LegacyAction::Permit2(p) => super::signature::permit2_request(p, host.clock().now()),
        LegacyAction::Eip2612(p) => super::signature::eip2612_request(p, host.clock().now()),
        LegacyAction::Eip712Other(o) => Ok(super::signature::eip712_other_request(
            o,
            host.clock().now(),
        )),
    }
}

/// Build one `PolicyRequest` from an action. The aggregate Dex action already
/// represents the full transaction-level intent.
///
/// # Errors
///
/// Returns an error when the action cannot be lowered without host
/// capabilities.
pub fn requests_from_action(action: &LegacyAction) -> Result<Vec<PolicyRequest>, PolicyError> {
    Ok(vec![request_from_action(action)?])
}

/// Build one policy request for each action in input order.
///
/// # Errors
///
/// Returns an error when any action cannot be lowered without host
/// capabilities.
pub fn requests_from_actions(actions: &[LegacyAction]) -> Result<Vec<PolicyRequest>, PolicyError> {
    actions.iter().map(request_from_action).collect()
}
