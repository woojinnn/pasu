//! `HyperliquidCore::HlUsdClassTransfer` lowering →
//! `HyperliquidCore::HlUsdClassTransferContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUsdClassTransferAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlUsdClassTransferAction` into the
/// `HyperliquidCore::HlUsdClassTransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlUsdClassTransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert("amount".into(), Value::String(action.amount.0.clone()));
    m.insert("toPerp".into(), Value::Bool(action.to_perp));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlUsdClassTransfer""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{
        HlUsdClassTransferAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn usd_class_transfer_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::UsdClassTransfer(
            HlUsdClassTransferAction {
                amount: Decimal::new("100.5"),
                to_perp: true,
            },
        ));
        assert_conforms("hl_usd_class_transfer", &body, &offchain_meta());
    }
}
