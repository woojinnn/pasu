//! UR command 0x05 TRANSFER — `(address token, address recipient, uint256 value)`.
//!
//! Sends `value` of `token` from the router to `recipient`. Unlike SWEEP
//! (which transfers whatever balance is left), TRANSFER pays an exact
//! amount. Modelled as `TransferAction(from = router, to = recipient,
//! amount = Exact(value))` so the simulator sees the precise router→user
//! settlement.

use policy_engine::action::misc::TransferAction;
use policy_engine::action::{Action, ActionEnvelope, AmountConstraint, AmountKind, Category};

use super::super::common::{
    asset_ref, asset_with_amount, map_recipient, native_asset, read_address_word, read_decimal_word,
};
use crate::{AdapterError, CallContext};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    input: &[u8],
) -> Result<ActionEnvelope, AdapterError> {
    let token = read_address_word(input, 0)?;
    let recipient = map_recipient(ctx, read_address_word(input, 1)?);
    let amount = read_decimal_word(input, 2)?;

    let asset = if is_zero_address(&token) {
        native_asset(ctx)
    } else {
        asset_ref(ctx, &token)
    };

    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Transfer(TransferAction {
            token: asset_with_amount(
                asset,
                AmountConstraint {
                    kind: AmountKind::Exact,
                    value: Some(amount),
                },
            ),
            from: ctx.to.clone(),
            recipient,
        }),
    })
}

fn is_zero_address(addr: &policy_engine::action::Address) -> bool {
    addr.to_string().to_ascii_lowercase() == "0x0000000000000000000000000000000000000000"
}
