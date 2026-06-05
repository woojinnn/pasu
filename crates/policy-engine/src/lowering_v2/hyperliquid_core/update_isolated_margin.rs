//! `HyperliquidCore::HlUpdateIsolatedMargin` lowering →
//! `HyperliquidCore::HlUpdateIsolatedMarginContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUpdateIsolatedMarginAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{hl_market, hl_venue};

/// Lower an `HlUpdateIsolatedMarginAction` into the
/// `HyperliquidCore::HlUpdateIsolatedMarginContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlUpdateIsolatedMarginAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "market".into(),
        hl_market(action.asset_index, action.symbol.as_deref()),
    );
    m.insert("isBuy".into(), Value::Bool(action.is_buy));
    m.insert("ntli".into(), Value::String(action.ntli.0.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlUpdateIsolatedMargin""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{
        HlUpdateIsolatedMarginAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn update_isolated_margin_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::UpdateIsolatedMargin(
            HlUpdateIsolatedMarginAction {
                asset_index: 1,
                symbol: Some("ETH".to_owned()),
                is_buy: true,
                ntli: Decimal::new("-100"),
            },
        ));
        assert_conforms("hl_update_isolated_margin", &body, &offchain_meta());
    }
}
