//! `HyperliquidCore::HlTwapOrder` lowering → `HyperliquidCore::HlTwapOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlTwapOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{hl_market, hl_venue};

/// Lower an `HlTwapOrderAction` into the `HyperliquidCore::HlTwapOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlTwapOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "market".into(),
        hl_market(action.asset_index, action.symbol.as_deref()),
    );
    m.insert(
        "side".into(),
        Value::String(if action.is_buy { "long" } else { "short" }.into()),
    );
    m.insert("size".into(), Value::String(action.size.0.clone()));
    m.insert("reduceOnly".into(), Value::Bool(action.reduce_only));
    // Derived intent (see `order.rs`): reduce-only ⇒ "reduce" (closes/shrinks an
    // existing position), else "open". Lets a "no new shorts" policy match
    // `side=="short" && positionEffect=="open"` without blocking long-closes.
    m.insert(
        "positionEffect".into(),
        Value::String(if action.reduce_only { "reduce" } else { "open" }.into()),
    );
    m.insert("minutes".into(), Value::from(i64::from(action.minutes)));
    m.insert("randomize".into(), Value::Bool(action.randomize));
    // Effective per-asset leverage, host-enriched (the wire has none). Emitted
    // ONLY when injected; absent ⇒ the optional schema field is omitted and a
    // `context has leverage` policy stays dormant. A TWAP carries the same
    // leveraged exposure as a regular order, so it must be enriched too (else
    // an order-leverage cap on HlOrder is evaded by routing exposure via TWAP).
    if let Some(leverage) = ctx.leverage_for(action.asset_index) {
        m.insert("leverage".into(), Value::from(leverage));
    }

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlTwapOrder""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{HlTwapOrderAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn twap_order_lowering_conforms_to_schema() {
        let body =
            ActionBody::HyperliquidCore(HyperliquidCoreAction::TwapOrder(HlTwapOrderAction {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_buy: true,
                size: Decimal::new("10"),
                reduce_only: false,
                minutes: 30,
                randomize: true,
            }));
        assert_conforms("hl_twap_order", &body, &offchain_meta());
    }

    /// `positionEffect` mirrors `order.rs`: derived from `reduce_only`, orthogonal
    /// to `side`. A reduce-only sell TWAP is `side=="short"` / effect `"reduce"`.
    #[test]
    fn twap_position_effect_derives_from_reduce_only() {
        use crate::lowering_v2::{lower_action, TxMeta};
        let tx = TxMeta {
            from: "0x1111111111111111111111111111111111111111",
            to: "0x2222222222222222222222222222222222222222",
        };
        let make = |reduce_only: bool| {
            ActionBody::HyperliquidCore(HyperliquidCoreAction::TwapOrder(HlTwapOrderAction {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_buy: false, // sell ⇒ side == "short"
                size: Decimal::new("10"),
                reduce_only,
                minutes: 30,
                randomize: true,
            }))
        };
        let open = lower_action(&make(false), &offchain_meta(), &tx).unwrap();
        assert_eq!(open.context["side"], "short");
        assert_eq!(open.context["positionEffect"], "open");
        let reduce = lower_action(&make(true), &offchain_meta(), &tx).unwrap();
        assert_eq!(reduce.context["side"], "short");
        assert_eq!(reduce.context["positionEffect"], "reduce");
    }

    /// Host-injected leverage surfaces as `context.leverage` (Long) on a TWAP and
    /// the enriched context conforms — closing the TWAP bypass of the
    /// order-leverage policy. Without injection the field is omitted.
    #[test]
    fn twap_with_injected_leverage_emits_long_and_conforms() {
        use crate::lowering_v2::{lower_action_enriched, AccountLeverage, TokenDecimals, TxMeta};

        let body =
            ActionBody::HyperliquidCore(HyperliquidCoreAction::TwapOrder(HlTwapOrderAction {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_buy: true,
                size: Decimal::new("10"),
                reduce_only: false,
                minutes: 30,
                randomize: true,
            }));
        let meta = offchain_meta();
        let tx = TxMeta {
            from: "0x1111111111111111111111111111111111111111",
            to: "0x2222222222222222222222222222222222222222",
        };
        let mut map = std::collections::BTreeMap::new();
        map.insert("0".to_owned(), 26i64);
        let lev = AccountLeverage::new(map);

        let lowered =
            lower_action_enriched(&body, &meta, &tx, &TokenDecimals::default(), &lev).unwrap();
        assert_eq!(lowered.context["leverage"], 26);

        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": "hl_twap_order-schema",
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": "hl_twap_order" } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("enriched hl_twap_order context must conform: {e:?}"));
    }
}
