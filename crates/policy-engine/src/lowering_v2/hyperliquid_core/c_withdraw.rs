//! `HyperliquidCore::HlCWithdraw` lowering → `HyperliquidCore::HlCWithdrawContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlCWithdrawAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlCWithdrawAction` into the `HyperliquidCore::HlCWithdrawContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlCWithdrawAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert("wei".into(), Value::String(action.wei.0.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlCWithdraw""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{HlCWithdrawAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn c_withdraw_lowering_conforms_to_schema() {
        let body =
            ActionBody::HyperliquidCore(HyperliquidCoreAction::CWithdraw(HlCWithdrawAction {
                wei: Decimal::new("100000000000"),
            }));
        assert_conforms("hl_c_withdraw", &body, &offchain_meta());
    }
}
