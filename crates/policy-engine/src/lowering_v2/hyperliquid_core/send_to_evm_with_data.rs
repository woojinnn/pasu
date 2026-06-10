//! `HyperliquidCore::HlSendToEvmWithData` lowering → `Core::UnknownContext`.
//!
//! An arbitrary-calldata bridge to an EVM recipient. Lowers to the generic
//! `Core::Action::"Unknown"` preserving the risk-bearing `(target, calldata)`:
//! `target` = destinationRecipient, `calldata` = data, `value` = amount.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSendToEvmWithDataAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::unknown::HL_SENTINEL_CHAIN;

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSendToEvmWithDataAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert(
        "target".into(),
        Value::String(addr(&action.destination_recipient)),
    );
    m.insert("chain".into(), Value::String(HL_SENTINEL_CHAIN.into()));
    m.insert("calldata".into(), Value::String(action.data.clone()));
    m.insert("value".into(), Value::String(action.amount.0.clone()));

    Ok(ctx.lowered(r#"Core::Action::"Unknown""#, Value::Object(m)))
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
    fn send_to_evm_with_data_lowering_conforms_to_core_unknown_schema() {
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
        // Phase 0.2: HL action's own tag; resolves Core::Unknown after the repoint.
        assert_conforms("hl_send_to_evm_with_data", &body, &offchain_meta());
    }
}
