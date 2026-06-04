//! `Marketplace::SignOrder` lowering → `Marketplace::SignOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::marketplace::SignOrderAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{is_zero_bytes, lower_market_items, lower_marketplace_venue};

/// Lower a `Marketplace::SignOrder` action (off-chain Seaport order signature).
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &SignOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_marketplace_venue(&action.venue));
    m.insert("offerer".into(), Value::String(addr(&action.offerer)));
    if let Some(zone) = &action.zone {
        m.insert("zone".into(), Value::String(addr(zone)));
    }
    m.insert("offer".into(), lower_market_items(&action.offer));
    m.insert(
        "consideration".into(),
        lower_market_items(&action.consideration),
    );
    m.insert("orderType".into(), Value::String(action.order_type.clone()));
    m.insert("startTime".into(), Value::from(action.start_time.as_unix()));
    m.insert("endTime".into(), Value::from(action.end_time.as_unix()));
    m.insert(
        "conduitKey".into(),
        Value::String(action.conduit_key.clone()),
    );
    m.insert(
        "usesConduit".into(),
        Value::Bool(!is_zero_bytes(&action.conduit_key)),
    );
    m.insert("counter".into(), Value::String(u256_hex(action.counter)));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Marketplace::Action::"SignOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::marketplace::{MarketplaceAction, SignOrderAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, criteria_offer_item, native_consideration_item, nft_offer_item,
        offchain_meta, seaport_venue,
    };

    /// A standard listing (concrete NFT offered for ETH proceeds + a fee leg).
    #[test]
    fn sign_order_listing_conforms() {
        let body = ActionBody::Marketplace(MarketplaceAction::SignOrder(SignOrderAction {
            venue: seaport_venue(),
            offerer: super::super::test_support::submitter_addr(),
            zone: None,
            offer: vec![nft_offer_item()],
            consideration: vec![
                native_consideration_item(0, "0x000000000000000000000000000000000000a01c"),
                native_consideration_item(1, "0x0000a26b00c1f0df003000390027140000faa719"),
            ],
            order_type: "full_open".into(),
            start_time: policy_state::primitives::Time::from_unix(1_738_000_000),
            end_time: policy_state::primitives::Time::from_unix(1_738_100_000),
            conduit_key: "0x0000007b02230091a7ed01230072f7006a004d60a8d4e71d599b8104250f0000"
                .into(),
            counter: U256::ZERO,
        }));
        assert_conforms("sign_order", &body, &offchain_meta());
    }

    /// A collection-wide criteria OFFER (any token) — the `anyToken` drainer
    /// case — with a restricted zone.
    #[test]
    fn sign_order_criteria_with_zone_conforms() {
        let body = ActionBody::Marketplace(MarketplaceAction::SignOrder(SignOrderAction {
            venue: seaport_venue(),
            offerer: super::super::test_support::submitter_addr(),
            zone: Some(
                policy_state::primitives::Address::from_str(
                    "0x004c00500000ad104d7dbd00e3ae0a5c00560c00",
                )
                .unwrap(),
            ),
            offer: vec![criteria_offer_item()],
            consideration: vec![native_consideration_item(
                0,
                "0x000000000000000000000000000000000000a01c",
            )],
            order_type: "full_restricted".into(),
            start_time: policy_state::primitives::Time::from_unix(1_738_000_000),
            end_time: policy_state::primitives::Time::from_unix(1_999_999_999),
            conduit_key: "0x0000000000000000000000000000000000000000000000000000000000000000"
                .into(),
            counter: U256::from(5u64),
        }));
        assert_conforms("sign_order", &body, &offchain_meta());
    }

    use std::str::FromStr;
}
