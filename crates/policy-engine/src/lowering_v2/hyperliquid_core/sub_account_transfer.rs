//! `HyperliquidCore::HlSubAccountTransfer` lowering → `Token::Erc20TransferContext`.
//!
//! A USDC move between the master account and a Hyperliquid sub-account. Lowers
//! to the generic `Token::Action::"Erc20Transfer"`: `recipient` = sub-account
//! address, `token` = HL USDC, `amount` = raw 6-dp USDC. The `isDeposit`
//! direction is dropped (not representable in `Erc20Transfer`); the rewritten
//! confirm policy warns on both directions.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlSubAccountTransferAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::amount::hl_amount_projection;
use super::{hl_usdc_token_ref, HL_USDC_DECIMALS};

/// Lower an `HlSubAccountTransferAction` into the `Token::Erc20TransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlSubAccountTransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let p = hl_amount_projection(&action.usd, HL_USDC_DECIMALS);
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), hl_usdc_token_ref());
    m.insert(
        "recipient".into(),
        Value::String(addr(&action.sub_account_user)),
    );
    m.insert("amount".into(), Value::String(p.raw_hex));
    if let Some(nano) = p.nano {
        m.insert("amountNano".into(), Value::from(nano));
    }
    // `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"Erc20Transfer""#, Value::Object(m)))
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
    fn sub_account_transfer_lowering_conforms_to_erc20_transfer() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::SubAccountTransfer(
            HlSubAccountTransferAction {
                sub_account_user: Address::from_str("0x000000000000000000000000000000000000bEEF")
                    .unwrap(),
                is_deposit: false,
                usd: Decimal::new("75"),
            },
        ));
        assert_conforms("erc20_transfer", &body, &offchain_meta());
    }
}
