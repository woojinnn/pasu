//! UR command 0x00 V3_SWAP_EXACT_IN —
//! `(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)`.

use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind, Validity};

use crate::{AdapterError, CallContext};

use super::super::common::{
    amount_constraint, asset_ref, map_recipient, parse_v3_path, read_address_word, read_bool_word,
    read_decimal_word, read_dynamic_bytes, swap_envelope,
};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_in = read_decimal_word(input, 1)?;
    let amount_out_min = read_decimal_word(input, 2)?;
    let path = read_dynamic_bytes(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let parsed_path = parse_v3_path(path)?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, &parsed_path.token_in),
        token_out: asset_ref(ctx, &parsed_path.token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in),
        amount_out: amount_constraint(AmountKind::Min, amount_out_min),
        recipient,
        validity,
        fee_bps: parsed_path.fee_bps,
        enrichment: SwapEnrichment::default(),
    }))
}
