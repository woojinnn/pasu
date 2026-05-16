//! UR command 0x01 V3_SWAP_EXACT_OUT —
//! `(address recipient, uint256 amountOut, uint256 amountInMax, bytes path, bool payerIsUser)`.
//!
//! V3 exact-out paths are encoded in REVERSE order on Universal Router:
//! the path starts with the output token and ends with the input token,
//! because the swap router walks the path from the requested output side.
//! `parse_v3_path` always returns (first, fee, last) of the byte stream,
//! so for exact-out we flip the endpoints back into wallet-side semantics
//! (token_in = what the user spends, token_out = what they receive).

use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind, Validity};

use crate::{AdapterError, CallContext};

use super::super::common::{
    amount_constraint, asset_ref, asset_with_amount, map_recipient, parse_v3_path,
    read_address_word, read_bool_word, read_decimal_word, read_dynamic_bytes, swap_envelope,
};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_out = read_decimal_word(input, 1)?;
    let amount_in_max = read_decimal_word(input, 2)?;
    let path = read_dynamic_bytes(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let parsed_path = parse_v3_path(path)?;

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactOut,
        input_token: asset_with_amount(
            asset_ref(ctx, &parsed_path.token_out),
            amount_constraint(AmountKind::Max, amount_in_max),
        ),
        output_token: asset_with_amount(
            asset_ref(ctx, &parsed_path.token_in),
            amount_constraint(AmountKind::Exact, amount_out),
        ),
        recipient,
        validity,
        fee_bps: parsed_path.fee_bps,
    }))
}
