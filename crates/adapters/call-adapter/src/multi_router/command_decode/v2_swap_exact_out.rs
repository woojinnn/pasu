//! UR command 0x09 V2_SWAP_EXACT_OUT —
//! `(address recipient, uint256 amountOut, uint256 amountInMax, address[] path, bool payerIsUser)`.

use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind, Validity};

use crate::{AdapterError, CallContext};

use super::super::common::{
    amount_constraint, asset_ref, asset_with_amount, map_recipient, path_endpoints,
    read_address_word, read_bool_word, read_decimal_word, read_dynamic_address_array, swap_envelope,
};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_out = read_decimal_word(input, 1)?;
    let amount_in_max = read_decimal_word(input, 2)?;
    let path = read_dynamic_address_array(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let (token_in, token_out) = path_endpoints(&path, "v2")?;

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactOut,
        input_token: asset_with_amount(
            asset_ref(ctx, token_in),
            amount_constraint(AmountKind::Max, amount_in_max),
        ),
        output_token: asset_with_amount(
            asset_ref(ctx, token_out),
            amount_constraint(AmountKind::Exact, amount_out),
        ),
        recipient,
        validity,
        fee_bps: Some(30),
    }))
}
