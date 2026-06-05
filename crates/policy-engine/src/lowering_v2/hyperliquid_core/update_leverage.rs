//! `HyperliquidCore::HlUpdateLeverage` lowering →
//! `HyperliquidCore::HlUpdateLeverageContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUpdateLeverageAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{hl_market, hl_venue};

/// Lower an `HlUpdateLeverageAction` into the
/// `HyperliquidCore::HlUpdateLeverageContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlUpdateLeverageAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "market".into(),
        hl_market(action.asset_index, action.symbol.as_deref()),
    );
    m.insert("isCross".into(), Value::Bool(action.is_cross));
    m.insert("leverage".into(), Value::from(i64::from(action.leverage)));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlUpdateLeverage""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_transition::action::hyperliquid_core::{
        HlUpdateLeverageAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn update_leverage_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::UpdateLeverage(
            HlUpdateLeverageAction {
                asset_index: 1,
                symbol: Some("ETH".to_owned()),
                is_cross: false,
                leverage: 25,
            },
        ));
        assert_conforms("hl_update_leverage", &body, &offchain_meta());
    }
}
