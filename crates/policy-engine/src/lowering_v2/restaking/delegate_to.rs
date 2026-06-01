//! `Restaking::DelegateTo` lowering → `Restaking::DelegateToContext`.

use serde_json::{Map, Value};

use policy_transition::action::restaking::DelegateToAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::DelegateTo` action. No live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &DelegateToAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
    m.insert("operator".into(), Value::String(addr(&action.operator)));
    m.insert(
        "approverSalt".into(),
        Value::String(action.approver_salt.clone()),
    );

    Ok(ctx.lowered(r#"Restaking::Action::"DelegateTo""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::restaking::{DelegateToAction, RestakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other};

    #[test]
    fn delegate_to_conforms() {
        let body = ActionBody::Restaking(RestakingAction::DelegateTo(DelegateToAction {
            venue: eigenlayer_venue(),
            operator: other(),
            approver_salt: "0x".to_string() + &"0".repeat(64),
        }));
        super::super::test_support::assert_conforms("delegate_to", &body, &onchain_meta());
    }
}
