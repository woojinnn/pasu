//! `HyperliquidCore::HlCDeposit` lowering → `HyperliquidCore::HlCDepositContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlCDepositAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlCDepositAction` into the `HyperliquidCore::HlCDepositContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlCDepositAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert("wei".into(), Value::String(action.wei.0.clone()));

    Ok(ctx.lowered(r#"HyperliquidCore::Action::"HlCDeposit""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{HlCDepositAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn c_deposit_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::CDeposit(HlCDepositAction {
            wei: Decimal::new("100000000000"),
        }));
        assert_conforms("hl_c_deposit", &body, &offchain_meta());
    }
}
