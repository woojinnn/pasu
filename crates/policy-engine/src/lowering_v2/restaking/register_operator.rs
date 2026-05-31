//! `Restaking::RegisterOperator` lowering → `Restaking::RegisterOperatorContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::restaking::RegisterOperatorAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::RegisterOperator` action. No live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &RegisterOperatorAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
    m.insert(
        "delegationApprover".into(),
        Value::String(addr(&action.delegation_approver)),
    );
    m.insert(
        "allocationDelay".into(),
        Value::from(action.allocation_delay),
    );
    m.insert(
        "metadataUri".into(),
        Value::String(action.metadata_uri.clone()),
    );

    Ok(ctx.lowered(r#"Restaking::Action::"RegisterOperator""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::restaking::{RegisterOperatorAction, RestakingAction};
    use simulation_reducer::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other};

    #[test]
    fn register_operator_conforms() {
        let body =
            ActionBody::Restaking(RestakingAction::RegisterOperator(RegisterOperatorAction {
                venue: eigenlayer_venue(),
                delegation_approver: other(),
                allocation_delay: 0,
                metadata_uri: "https://example.com/operator.json".into(),
            }));
        super::super::test_support::assert_conforms("register_operator", &body, &onchain_meta());
    }
}
