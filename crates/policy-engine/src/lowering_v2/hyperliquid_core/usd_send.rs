//! `HyperliquidCore::HlUsdSend` lowering → `Token::Erc20TransferContext`.
//!
//! A USDC transfer off the Hyperliquid account to an arbitrary destination.
//! Lowers to the generic `Token::Action::"Erc20Transfer"` so existing
//! transfer-shaped policies (recipient confirm / sanctions / reputation) cover
//! it: `recipient` = destination, `token` = HL USDC, `amount` = raw 6-dp USDC.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUsdSendAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::amount::hl_amount_projection;
use super::{hl_usdc_token_ref, HL_USDC_DECIMALS};

/// Lower an `HlUsdSendAction` into the `Token::Erc20TransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlUsdSendAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let p = hl_amount_projection(&action.amount, HL_USDC_DECIMALS);
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), hl_usdc_token_ref());
    m.insert("recipient".into(), Value::String(addr(&action.destination)));
    m.insert("amount".into(), Value::String(p.raw_hex));
    if let Some(nano) = p.nano {
        m.insert("amountNano".into(), Value::from(nano));
    }
    // `amountUsd` / `custom` are host-populated — OMITTED here (matches every
    // `Token::*` leaf; the slot is Cedar `decimal`, not a String).

    Ok(ctx.lowered(r#"Token::Action::"Erc20Transfer""#, Value::Object(m)))
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
    fn usd_send_lowering_conforms_to_erc20_transfer() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::UsdSend(HlUsdSendAction {
            destination: Address::from_str("0x000000000000000000000000000000000000bEEF").unwrap(),
            amount: Decimal::new("250"),
        }));
        // After Task 2.3 repoints the ("hyperliquid_core","hl_usd_send")
        // RESOLVER_TABLE row to TOKEN_ERC20_TRANSFER_SCHEMA, composing the
        // destination `erc20_transfer` schema validates the lowered uid.
        assert_conforms("erc20_transfer", &body, &offchain_meta());
    }
}
