//! UR command 0x0b WRAP_ETH —
//! `(address recipient, uint256 amountMin)`. Native ETH → WETH mint to recipient.

use policy_engine::action::misc::WrapAction;
use policy_engine::action::{Action, ActionEnvelope, AmountConstraint, AmountKind, Category};

use crate::{AdapterError, CallContext};

use super::super::common::{
    asset_with_amount, map_recipient, native_asset, read_address_word, read_decimal_word, weth_asset,
};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    input: &[u8],
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_min = read_decimal_word(input, 1)?;
    let amount = AmountConstraint {
        kind: AmountKind::Min,
        value: Some(amount_min),
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Wrap(WrapAction {
            native_asset: asset_with_amount(native_asset(ctx), amount.clone()),
            wrapped_asset: asset_with_amount(weth_asset(ctx), amount),
            recipient,
        }),
    })
}
