//! `HyperliquidCore::HlSubAccountTransfer` lowering →
//! `HyperliquidCore::HlSubAccountTransferContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSubAccountTransferAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlSubAccountTransferAction` into the
/// `HyperliquidCore::HlSubAccountTransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSubAccountTransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "subAccountUser".into(),
        Value::String(addr(&action.sub_account_user)),
    );
    m.insert("isDeposit".into(), Value::Bool(action.is_deposit));
    m.insert("usd".into(), Value::String(action.usd.0.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlSubAccountTransfer""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{
        HlSubAccountTransferAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn sub_account_transfer_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::SubAccountTransfer(
            HlSubAccountTransferAction {
                sub_account_user: Address::from_str("0x000000000000000000000000000000000000bEEF")
                    .unwrap(),
                is_deposit: false,
                usd: Decimal::new("75"),
            },
        ));
        assert_conforms("hl_sub_account_transfer", &body, &offchain_meta());
    }
}
