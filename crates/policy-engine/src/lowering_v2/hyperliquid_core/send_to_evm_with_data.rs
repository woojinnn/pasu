//! `HyperliquidCore::HlSendToEvmWithData` lowering →
//! `HyperliquidCore::HlSendToEvmWithDataContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSendToEvmWithDataAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlSendToEvmWithDataAction` into the
/// `HyperliquidCore::HlSendToEvmWithDataContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSendToEvmWithDataAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert("token".into(), Value::String(action.token.clone()));
    m.insert("amount".into(), Value::String(action.amount.0.clone()));
    m.insert("sourceDex".into(), Value::String(action.source_dex.clone()));
    m.insert(
        "destinationRecipient".into(),
        Value::String(addr(&action.destination_recipient)),
    );
    m.insert("data".into(), Value::String(action.data.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlSendToEvmWithData""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{
        HlSendToEvmWithDataAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn send_to_evm_with_data_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::SendToEvmWithData(
            HlSendToEvmWithDataAction {
                token: "USDC".to_owned(),
                amount: Decimal::new("1000"),
                source_dex: String::new(),
                destination_recipient: Address::from_str(
                    "0x000000000000000000000000000000000000bEEF",
                )
                .unwrap(),
                data: "0xdeadbeef".to_owned(),
            },
        ));
        assert_conforms("hl_send_to_evm_with_data", &body, &offchain_meta());
    }
}
