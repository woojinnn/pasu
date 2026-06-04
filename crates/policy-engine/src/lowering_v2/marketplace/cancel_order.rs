//! `Marketplace::CancelOrder` lowering → `Marketplace::CancelOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::marketplace::CancelOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_marketplace_venue;

/// Lower a `Marketplace::CancelOrder` action (Seaport `cancel` /
/// `incrementCounter`).
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &CancelOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_marketplace_venue(&action.venue));
    m.insert("scope".into(), Value::String(action.scope.clone()));
    if let Some(order_count) = action.order_count {
        m.insert("orderCount".into(), Value::from(order_count));
    }

    Ok(ctx.lowered(r#"Marketplace::Action::"CancelOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::marketplace::{CancelOrderAction, MarketplaceAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, onchain_meta, seaport_venue};

    /// Cancel two specific orders (scope="orders", orderCount=2).
    #[test]
    fn cancel_specific_orders_conforms() {
        let body = ActionBody::Marketplace(MarketplaceAction::CancelOrder(CancelOrderAction {
            venue: seaport_venue(),
            scope: "orders".into(),
            order_count: Some(2),
        }));
        assert_conforms("cancel_order", &body, &onchain_meta());
    }

    /// incrementCounter — cancel ALL (scope="all", orderCount absent).
    #[test]
    fn cancel_all_conforms() {
        let body = ActionBody::Marketplace(MarketplaceAction::CancelOrder(CancelOrderAction {
            venue: seaport_venue(),
            scope: "all".into(),
            order_count: None,
        }));
        assert_conforms("cancel_order", &body, &onchain_meta());
    }
}
