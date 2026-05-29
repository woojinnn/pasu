//! `Perp::CancelOrder` lowering → `Perp::CancelOrderContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::CancelOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_perp_venue;

/// Lower a `CancelOrderAction` into the `Perp::CancelOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &CancelOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("orderId".into(), Value::String(action.order_id.clone()));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"CancelOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{CancelOrderAction, PerpAction};
    use simulation_reducer::action::ActionBody;

    use super::super::test_support::{assert_conforms, offchain_meta, sample_venue};

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = CancelOrderAction {
            venue: sample_venue(),
            order_id: "order-0xdeadbeef".into(),
        };
        (
            ActionBody::Perp(PerpAction::CancelOrder(action)),
            offchain_meta(),
        )
    }

    #[test]
    fn cancel_order_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("cancel_order", &body, &meta);
    }
}
