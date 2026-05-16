//! UR command 0x04 SWEEP — `(address token, address recipient, uint256 amountMin)`.
//!
//! Drains the router's balance of `token` to `recipient`, requiring at least
//! `amountMin`. Routers use this as the final settlement step after a swap
//! lands its output on the router (recipient = `ACTION_ADDRESS_THIS`).
//!
//! Modelled as `TransferAction(from = router, to = recipient,
//! amount = AtLeast(amountMin))`. The simulator then sees the router→user
//! settlement and can collapse a `[WRAP, SWAP, SWEEP]` sequence back to a
//! single `Swap(ETH → output)`.

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
    let amount_min = read_decimal_word(input, 2)?;

    // Zero address sentinel = native ETH; anything else is an ERC-20.
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
                    kind: AmountKind::Min,
                    value: Some(amount_min),
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
