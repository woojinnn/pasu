//! `Marketplace::FulfillOrder` lowering → `Marketplace::FulfillOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::marketplace::FulfillOrderAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{is_zero_bytes, lower_market_items, lower_marketplace_venue};

/// Lower a `Marketplace::FulfillOrder` action (on-chain Seaport fulfill/match).
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &FulfillOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_marketplace_venue(&action.venue));
    m.insert("offer".into(), lower_market_items(&action.offer));
    m.insert(
        "consideration".into(),
        lower_market_items(&action.consideration),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    let uses_conduit = action
        .fulfiller_conduit_key
        .as_ref()
        .is_some_and(|k| !is_zero_bytes(k));
    if let Some(key) = &action.fulfiller_conduit_key {
        m.insert("fulfillerConduitKey".into(), Value::String(key.clone()));
    }
    m.insert("usesConduit".into(), Value::Bool(uses_conduit));
    m.insert("orderCount".into(), Value::from(action.order_count));
    m.insert("isBatch".into(), Value::Bool(action.is_batch));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Marketplace::Action::"FulfillOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::marketplace::{FulfillOrderAction, MarketplaceAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, native_consideration_item, nft_offer_item, onchain_meta, seaport_venue,
        submitter_addr,
    };

    /// A single buy: taker receives an NFT, pays ETH proceeds + a fee leg.
    #[test]
    fn fulfill_single_buy_conforms() {
        let body = ActionBody::Marketplace(MarketplaceAction::FulfillOrder(FulfillOrderAction {
            venue: seaport_venue(),
            offer: vec![nft_offer_item()],
            consideration: vec![
                native_consideration_item(0, "0x000000000000000000000000000000000000a01c"),
                native_consideration_item(1, "0x0000a26b00c1f0df003000390027140000faa719"),
            ],
            recipient: submitter_addr(),
            fulfiller_conduit_key: None,
            order_count: 1,
            is_batch: false,
        }));
        assert_conforms("fulfill_order", &body, &onchain_meta());
    }

    /// A batch sweep with a non-zero fulfiller conduit key (usesConduit=true).
    #[test]
    fn fulfill_batch_with_conduit_conforms() {
        let body = ActionBody::Marketplace(MarketplaceAction::FulfillOrder(FulfillOrderAction {
            venue: seaport_venue(),
            offer: vec![nft_offer_item(), nft_offer_item()],
            consideration: vec![native_consideration_item(
                0,
                "0x000000000000000000000000000000000000a01c",
            )],
            recipient: submitter_addr(),
            fulfiller_conduit_key: Some(
                "0x0000007b02230091a7ed01230072f7006a004d60a8d4e71d599b8104250f0000".into(),
            ),
            order_count: 2,
            is_batch: true,
        }));
        assert_conforms("fulfill_order", &body, &onchain_meta());
    }
}
