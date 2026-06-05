//! `HyperliquidCore::HlOrder` lowering → `HyperliquidCore::HlOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{hl_market, hl_venue};

/// Lower an `HlOrderAction` into the `HyperliquidCore::HlOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlOrderAction,
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
    m.insert("price".into(), Value::String(action.price.0.clone()));
    m.insert("size".into(), Value::String(action.size.0.clone()));
    m.insert("reduceOnly".into(), Value::Bool(action.reduce_only));
    // Derived intent: an order's `side` (isBuy) is its DIRECTION (buy/sell), NOT
    // its position effect. A reduce-only order can only shrink an existing
    // position (venue-enforced), so a reduce-only "short" (sell) is a long-CLOSE,
    // not a new short. `positionEffect` lets a "no new shorts" policy match
    // `side=="short" && positionEffect=="open"` and stop blocking long-closes.
    // Computed purely from `reduce_only` — no live position data.
    m.insert(
        "positionEffect".into(),
        Value::String(if action.reduce_only { "reduce" } else { "open" }.into()),
    );
    m.insert("tif".into(), Value::String(action.tif.clone()));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"HyperliquidCore::Action::"HlOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn order_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
            asset_index: 0,
            symbol: Some("BTC".to_owned()),
            is_buy: false,
            price: Decimal::new("60000"),
            size: Decimal::new("0.1"),
            reduce_only: false,
            tif: "gtc".to_owned(),
        }));
        assert_conforms("hl_order", &body, &offchain_meta());
    }

    /// An order with no resolved symbol still conforms (falls back to ASSET-<n>).
    #[test]
    fn order_without_symbol_conforms() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
            asset_index: 7,
            symbol: None,
            is_buy: true,
            price: Decimal::new("0.00001234"),
            size: Decimal::new("1000.5"),
            reduce_only: true,
            tif: "ioc".to_owned(),
        }));
        assert_conforms("hl_order", &body, &offchain_meta());
    }

    /// `positionEffect` is derived from `reduce_only` and is ORTHOGONAL to `side`:
    /// a reduce-only sell is `side=="short"` but `positionEffect=="reduce"` (a
    /// long-CLOSE), which is exactly what stops a "no new shorts" policy from
    /// blocking position exits.
    #[test]
    fn order_position_effect_derives_from_reduce_only() {
        use crate::lowering_v2::{lower_action, TxMeta};
        let tx = TxMeta {
            from: "0x1111111111111111111111111111111111111111",
            to: "0x2222222222222222222222222222222222222222",
        };
        let make = |reduce_only: bool| {
            ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_buy: false, // sell ⇒ side == "short"
                price: Decimal::new("60000"),
                size: Decimal::new("0.1"),
                reduce_only,
                tif: "gtc".to_owned(),
            }))
        };

        // New short (open): side "short" + positionEffect "open".
        let open = lower_action(&make(false), &offchain_meta(), &tx).unwrap();
        assert_eq!(open.context["side"], "short");
        assert_eq!(open.context["positionEffect"], "open");

        // Reduce-only sell = long-CLOSE: side still "short" but effect "reduce".
        let reduce = lower_action(&make(true), &offchain_meta(), &tx).unwrap();
        assert_eq!(reduce.context["side"], "short");
        assert_eq!(reduce.context["positionEffect"], "reduce");
    }
}
