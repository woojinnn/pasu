//! `HyperliquidCore::HlUsdSend` lowering → `HyperliquidCore::HlUsdSendContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUsdSendAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlUsdSendAction` into the `HyperliquidCore::HlUsdSendContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlUsdSendAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "destination".into(),
        Value::String(addr(&action.destination)),
    );
    m.insert("amount".into(), Value::String(action.amount.0.clone()));

    Ok(ctx.lowered(r#"HyperliquidCore::Action::"HlUsdSend""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{HlUsdSendAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn usd_send_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::UsdSend(HlUsdSendAction {
            destination: Address::from_str("0x000000000000000000000000000000000000bEEF").unwrap(),
            amount: Decimal::new("250"),
        }));
        assert_conforms("hl_usd_send", &body, &offchain_meta());
    }
}
