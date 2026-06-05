//! `Restaking::Redelegate` lowering → `Restaking::RedelegateContext`.

use serde_json::{Map, Value};

use policy_transition::action::restaking::RedelegateAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::Redelegate` action. No live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &RedelegateAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
    m.insert(
        "newOperator".into(),
        Value::String(addr(&action.new_operator)),
    );
    m.insert(
        "approverSalt".into(),
        Value::String(action.approver_salt.clone()),
    );

    Ok(ctx.lowered(r#"Restaking::Action::"Redelegate""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::restaking::{RedelegateAction, RestakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other};

    #[test]
    fn redelegate_conforms() {
        let body = ActionBody::Restaking(RestakingAction::Redelegate(RedelegateAction {
            venue: eigenlayer_venue(),
            new_operator: other(),
            approver_salt: "0x".to_string() + &"0".repeat(64),
        }));
        super::super::test_support::assert_conforms("redelegate", &body, &onchain_meta());
    }
}
