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
}
