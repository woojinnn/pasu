//! `HyperliquidCore::HlSpotSend` lowering → `HyperliquidCore::HlSpotSendContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSpotSendAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlSpotSendAction` into the `HyperliquidCore::HlSpotSendContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSpotSendAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "destination".into(),
        Value::String(addr(&action.destination)),
    );
    m.insert("token".into(), Value::String(action.token.clone()));
    m.insert("amount".into(), Value::String(action.amount.0.clone()));

    Ok(ctx.lowered(r#"HyperliquidCore::Action::"HlSpotSend""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{HlSpotSendAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn spot_send_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::SpotSend(HlSpotSendAction {
            destination: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
            token: "USDC:0xc1fb593aeffbeb02f85e0308e9956a90".to_owned(),
            amount: Decimal::new("500.25"),
        }));
        assert_conforms("hl_spot_send", &body, &offchain_meta());
    }
}
