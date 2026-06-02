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
}
