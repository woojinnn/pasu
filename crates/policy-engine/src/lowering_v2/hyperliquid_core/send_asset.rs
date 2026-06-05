//! `HyperliquidCore::HlSendAsset` lowering → `HyperliquidCore::HlSendAssetContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSendAssetAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlSendAssetAction` into the `HyperliquidCore::HlSendAssetContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSendAssetAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "destination".into(),
        Value::String(addr(&action.destination)),
    );
    m.insert("sourceDex".into(), Value::String(action.source_dex.clone()));
    m.insert(
        "destinationDex".into(),
        Value::String(action.destination_dex.clone()),
    );
    m.insert("token".into(), Value::String(action.token.clone()));
    m.insert("amount".into(), Value::String(action.amount.0.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlSendAsset""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{HlSendAssetAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn send_asset_lowering_conforms_to_schema() {
        let body =
            ActionBody::HyperliquidCore(HyperliquidCoreAction::SendAsset(HlSendAssetAction {
                destination: Address::from_str("0x000000000000000000000000000000000000bEEF")
                    .unwrap(),
                source_dex: String::new(),
                destination_dex: "perp".to_owned(),
                token: "USDC".to_owned(),
                amount: Decimal::new("25"),
            }));
        assert_conforms("hl_send_asset", &body, &offchain_meta());
    }
}
