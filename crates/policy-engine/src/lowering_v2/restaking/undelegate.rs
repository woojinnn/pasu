//! `Restaking::Undelegate` lowering → `Restaking::UndelegateContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::restaking::UndelegateAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::Undelegate` action. No live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &UndelegateAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
    m.insert("staker".into(), Value::String(addr(&action.staker)));

    Ok(ctx.lowered(r#"Restaking::Action::"Undelegate""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::restaking::{RestakingAction, UndelegateAction};
    use simulation_reducer::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other};

    #[test]
    fn undelegate_conforms() {
        let body = ActionBody::Restaking(RestakingAction::Undelegate(UndelegateAction {
            venue: eigenlayer_venue(),
            staker: other(),
        }));
        super::super::test_support::assert_conforms("undelegate", &body, &onchain_meta());
    }
}
