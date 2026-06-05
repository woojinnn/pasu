//! `Perp::CancelOrder` lowering → `Perp::CancelOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::perp::CancelOrderAction;

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
    use policy_transition::action::perp::{CancelOrderAction, PerpAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{all_venues, assert_conforms, offchain_meta, sample_venue};

    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
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

    /// Drive **every** `PerpVenue` variant through `lower_perp_venue` end-to-end.
    /// `CancelOrder` is the leanest perp action (venue + `order_id` only), so it
    /// isolates the venue lowering: the 8 `{ name, chain }` arms plus the
    /// `Generic` `{ name, chain, contract }` arm must all conform.
    #[test]
    fn cancel_order_all_venue_variants_conform() {
        for (name, venue) in all_venues() {
            let action = CancelOrderAction {
                venue,
                order_id: format!("order-{name}"),
            };
            let body = ActionBody::Perp(PerpAction::CancelOrder(action));
            assert_conforms("cancel_order", &body, &offchain_meta());
        }
    }
}
