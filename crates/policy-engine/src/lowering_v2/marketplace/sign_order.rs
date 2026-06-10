//! `Marketplace::SignOrder` lowering → `Marketplace::SignOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::marketplace::SignOrderAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{
    is_any_token, is_criteria, is_zero_bytes, lower_market_items, lower_marketplace_venue,
};

/// Lower a decode-time unix timestamp (`Time` is a `u64`) into a Cedar `Long`
/// (`i64`), saturating at `i64::MAX`. A Seaport "never expires" order encodes
/// `endTime = type(uint256).max`, which the decoder saturates to `u64::MAX` —
/// out of `Long` range, which would fault context construction (dropping every
/// SignOrder policy). Clamping keeps the context valid and lets the far-future
/// expiry check correctly flag the infinite-listing case.
fn long_secs(secs: u64) -> Value {
    Value::from(secs.min(i64::MAX as u64))
}

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
    m.insert("startTime".into(), long_secs(action.start_time.as_unix()));
    m.insert("endTime".into(), long_secs(action.end_time.as_unix()));
    m.insert(
        "conduitKey".into(),
        Value::String(action.conduit_key.clone()),
    );
    m.insert(
        "usesConduit".into(),
        Value::Bool(!is_zero_bytes(&action.conduit_key)),
    );
    // Derived flatteners over the `Set<MarketItem>` offer/consideration (Cedar
    // cannot inspect set members, so the drainer signals are projected to base
    // bools here — same pattern as `usesConduit` / per-item `anyToken`).
    //
    // `offerHasCriteria`: any OFFER leg is a criteria item (regardless of Merkle
    // root) — the multi-token giveaway primitive. A human-signed maker listing
    // offers concrete tokenIds, so an offer-side criteria leg (zero-root = any
    // token, or non-zero root = a tree of the victim's tokenIds) is the giveaway
    // signal. (`offerHasAnyToken` is the strict zero-root sub-case, kept for a
    // deny-strict variant + the per-item `anyToken` parity.)
    m.insert(
        "offerHasCriteria".into(),
        Value::Bool(action.offer.iter().any(is_criteria)),
    );
    // `offerHasAnyToken`: any OFFER leg gives away ANY token in a collection
    // (zero-root criteria) — the strict whole-collection sub-case.
    m.insert(
        "offerHasAnyToken".into(),
        Value::Bool(action.offer.iter().any(is_any_token)),
    );
    // `proceedsToOfferer`: at least one CONSIDERATION leg is paid to the offerer
    // (the signer receives some proceeds). false ⇒ every payout is routed away
    // from the signer (proceeds-redirect drain).
    let offerer = addr(&action.offerer);
    m.insert(
        "proceedsToOfferer".into(),
        Value::Bool(
            action
                .consideration
                .iter()
                .any(|item| item.recipient.as_ref().is_some_and(|r| addr(r) == offerer)),
        ),
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

    /// Build a `SignOrder` with the given offer/consideration legs.
    fn sign_order(
        offer: Vec<policy_transition::action::marketplace::MarketItem>,
        consideration: Vec<policy_transition::action::marketplace::MarketItem>,
    ) -> ActionBody {
        ActionBody::Marketplace(MarketplaceAction::SignOrder(SignOrderAction {
            venue: seaport_venue(),
            offerer: super::super::test_support::submitter_addr(),
            zone: None,
            offer,
            consideration,
            order_type: "full_open".into(),
            start_time: policy_state::primitives::Time::from_unix(1_738_000_000),
            end_time: policy_state::primitives::Time::from_unix(1_738_100_000),
            conduit_key: "0x0000007b02230091a7ed01230072f7006a004d60a8d4e71d599b8104250f0000"
                .into(),
            counter: U256::ZERO,
        }))
    }

    fn lowered_ctx(body: &ActionBody) -> serde_json::Value {
        crate::lowering_v2::lower_action(
            body,
            &offchain_meta(),
            &crate::lowering_v2::TxMeta {
                from: super::super::test_support::FROM,
                to: super::super::test_support::TO,
            },
        )
        .unwrap()
        .context
    }

    /// DRAIN: offer is a zero-root criteria item (any NFT in collection) and the
    /// only consideration leg pays a THIRD party — the two load-bearing derived
    /// flatteners both fire.
    #[test]
    fn sign_order_drain_derives_giveaway_and_no_proceeds() {
        // offerer = submitter (0x1111…); consideration pays 0x…a01c (not self).
        let body = sign_order(
            vec![criteria_offer_item()],
            vec![native_consideration_item(
                0,
                "0x000000000000000000000000000000000000a01c",
            )],
        );
        let ctx = lowered_ctx(&body);
        assert_eq!(ctx["offerHasCriteria"], serde_json::json!(true));
        assert_eq!(ctx["offerHasAnyToken"], serde_json::json!(true));
        assert_eq!(ctx["proceedsToOfferer"], serde_json::json!(false));
    }

    /// DRAIN (non-zero Merkle root): an OFFER-side criteria item whose root is
    /// NON-zero (a tree over many of the victim's tokenIds) is still a multi-token
    /// giveaway — `offerHasCriteria` must fire even though `offerHasAnyToken`
    /// (the zero-root sub-case) does not. This is the false-negative the
    /// adversarial review caught.
    #[test]
    fn sign_order_nonzero_criteria_offer_flags_criteria_not_anytoken() {
        let body = sign_order(
            vec![{
                let mut item = criteria_offer_item();
                // a real, non-zero Merkle root over a set of tokenIds.
                item.criteria_root = Some(
                    "0xabc0000000000000000000000000000000000000000000000000000000000def".into(),
                );
                item
            }],
            vec![native_consideration_item(
                0,
                "0x000000000000000000000000000000000000a01c",
            )],
        );
        let ctx = lowered_ctx(&body);
        assert_eq!(
            ctx["offerHasCriteria"],
            serde_json::json!(true),
            "a non-zero-root offer-side criteria item is still a giveaway"
        );
        assert_eq!(
            ctx["offerHasAnyToken"],
            serde_json::json!(false),
            "non-zero root is not the strict any-token sub-case"
        );
    }

    /// `startTime`/`endTime` saturate to i64::MAX (not u64::MAX) so a uint256.max
    /// "never expires" endTime stays a valid Cedar Long instead of faulting.
    #[test]
    fn sign_order_max_end_time_clamps_to_i64_max() {
        let mut action = match sign_order(vec![nft_offer_item()], vec![]) {
            ActionBody::Marketplace(MarketplaceAction::SignOrder(a)) => a,
            _ => unreachable!(),
        };
        action.end_time = policy_state::primitives::Time::from_unix(u64::MAX);
        let ctx = lowered_ctx(&ActionBody::Marketplace(MarketplaceAction::SignOrder(
            action,
        )));
        assert_eq!(ctx["endTime"], serde_json::json!(i64::MAX));
    }

    /// LEGIT LISTING: concrete NFT offer; the first consideration leg pays the
    /// offerer (self) and a second leg is a marketplace fee to another address.
    /// Neither flattener fires (fee leg to a non-self recipient must NOT trip
    /// `proceedsToOfferer`).
    #[test]
    fn sign_order_legit_listing_derives_safe() {
        let body = sign_order(
            vec![nft_offer_item()],
            vec![
                // proceeds → offerer (submitter_addr = 0x1111…1111)
                native_consideration_item(0, "0x1111111111111111111111111111111111111111"),
                // OpenSea fee leg → fee wallet (recipient != offerer, legit)
                native_consideration_item(1, "0x0000a26b00c1f0df003000390027140000faa719"),
            ],
        );
        let ctx = lowered_ctx(&body);
        assert_eq!(ctx["offerHasCriteria"], serde_json::json!(false));
        assert_eq!(ctx["offerHasAnyToken"], serde_json::json!(false));
        assert_eq!(ctx["proceedsToOfferer"], serde_json::json!(true));
    }

    /// LEGIT COLLECTION BID: offerer gives WETH (a concrete ERC20, modelled here
    /// as a non-criteria offer leg) and the criteria item is on the
    /// CONSIDERATION side — `offerHasAnyToken` must stay false (the FP landmine).
    #[test]
    fn sign_order_collection_bid_not_flagged_as_giveaway() {
        let body = sign_order(
            vec![nft_offer_item()], // concrete offer leg (stand-in for WETH bid leg)
            vec![{
                // criteria (any-token) on the CONSIDERATION side, paid to offerer.
                let mut item = criteria_offer_item();
                item.recipient = Some(
                    policy_state::primitives::Address::from_str(
                        "0x1111111111111111111111111111111111111111",
                    )
                    .unwrap(),
                );
                item
            }],
        );
        let ctx = lowered_ctx(&body);
        assert_eq!(
            ctx["offerHasCriteria"],
            serde_json::json!(false),
            "criteria on the consideration side is a legit collection bid, not a giveaway"
        );
        assert_eq!(ctx["offerHasAnyToken"], serde_json::json!(false));
        assert_eq!(ctx["proceedsToOfferer"], serde_json::json!(true));
    }

    use std::str::FromStr;
}
