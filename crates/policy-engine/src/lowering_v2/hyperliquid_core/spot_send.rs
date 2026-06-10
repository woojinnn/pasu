//! `HyperliquidCore::HlSpotSend` lowering → `Token::Erc20TransferContext`.
//!
//! A spot-token transfer off the Hyperliquid account. Lowers to the generic
//! `Token::Action::"Erc20Transfer"`: `recipient` = destination, `token` = the
//! HL spot token id. Only USDC's 6-dp decimals are known statically; other
//! spot tokens are magnitude-blind (the `amount` slot is a type-honest `"0x0"`
//! so allowlist / sanctions / reputation policies still drive on token +
//! recipient while quantity caps stay dormant — see spec open item #1).

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSpotSendAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::amount::hl_amount_projection;
use super::{hl_token_ref, HL_USDC_DECIMALS};

/// Lower an `HlSpotSendAction` into the `Token::Erc20TransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSpotSendAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let is_usdc = action.token.starts_with("USDC");
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), hl_token_ref(&action.token));
    m.insert("recipient".into(), Value::String(addr(&action.destination)));
    if is_usdc {
        let p = hl_amount_projection(&action.amount, HL_USDC_DECIMALS);
        m.insert("amount".into(), Value::String(p.raw_hex));
        if let Some(nano) = p.nano {
            m.insert("amountNano".into(), Value::from(nano));
        }
    } else {
        // Decimals unknown for non-USDC spot tokens: the `amount` slot is raw
        // U256 hex everywhere, so emit a type-honest `"0x0"` rather than a
        // decimal string a numeric/hex read would mishandle.
        m.insert("amount".into(), Value::String("0x0".into()));
    }
    // `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"Erc20Transfer""#, Value::Object(m)))
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
    fn spot_send_lowering_conforms_to_erc20_transfer() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::SpotSend(HlSpotSendAction {
            destination: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
            token: "USDC:0xc1fb593aeffbeb02f85e0308e9956a90".to_owned(),
            amount: Decimal::new("500.25"),
        }));
        assert_conforms("erc20_transfer", &body, &offchain_meta());
    }

    #[test]
    fn spot_send_non_usdc_token_conforms() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::SpotSend(HlSpotSendAction {
            destination: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
            token: "PURR:0xabc".to_owned(),
            amount: Decimal::new("1000"),
        }));
        assert_conforms("erc20_transfer", &body, &offchain_meta());
    }
}
