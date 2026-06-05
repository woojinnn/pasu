//! `Yield::CancelLimitOrder` lowering → `Yield::CancelLimitOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::yield_::CancelLimitOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{enum_tag, lower_yield_venue};

/// Lower a `Yield::CancelLimitOrder` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &CancelLimitOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("kind".into(), enum_tag(&action.kind));

    Ok(ctx.lowered(r#"Yield::Action::"CancelLimitOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::yield_::{CancelKind, CancelLimitOrderAction, YieldAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, onchain_meta, pendle_venue};

    #[test]
    fn cancel_single_conforms() {
        let body = ActionBody::Yield(YieldAction::CancelLimitOrder(CancelLimitOrderAction {
            venue: pendle_venue(),
            kind: CancelKind::Single,
        }));
        assert_conforms("cancel_limit_order", &body, &onchain_meta());
    }

    #[test]
    fn cancel_batch_conforms() {
        let body = ActionBody::Yield(YieldAction::CancelLimitOrder(CancelLimitOrderAction {
            venue: pendle_venue(),
            kind: CancelKind::Batch,
        }));
        assert_conforms("cancel_limit_order", &body, &onchain_meta());
    }
}
