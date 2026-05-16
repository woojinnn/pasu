//! `V4Router.swapExactIn(params: ExactInputParams)` — multi-hop variant.
//! params: (currencyIn, path[], minHopPriceX36, amountIn, amountOutMinimum)

use abi_resolver::subdecode::opcode_stream::DecodedStep;
use alloy_dyn_abi::DynSolValue;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind, Validity};

use crate::{AdapterError, CallContext};

use super::super::common::{
    amount_constraint, asset_with_amount, decimal, swap_envelope, v4_asset_ref,
};
use super::{tuple_address, tuple_uint, v4_params_tuple};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    step: &DecodedStep,
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let fields = v4_params_tuple(step)?;
    let currency_in = tuple_address(&fields[0], "currencyIn")?;
    let path_val = &fields[1];
    let DynSolValue::Array(path_items) = path_val else {
        return Err(AdapterError::Invalid("V4 path not an array".into()));
    };
    let last = path_items
        .last()
        .ok_or_else(|| AdapterError::Invalid("V4 path empty".into()))?;
    let DynSolValue::Tuple(last_fields) = last else {
        return Err(AdapterError::Invalid("V4 path entry not tuple".into()));
    };
    let currency_out = tuple_address(&last_fields[0], "path.last.intermediateCurrency")?;

    let amount_in = tuple_uint(&fields[3], "amountIn")?;
    let amount_out_min = tuple_uint(&fields[4], "amountOutMinimum")?;
    let fee_bps = match &last_fields[1] {
        DynSolValue::Uint(u, _) => Some(u32::try_from(*u).unwrap_or(0) / 100),
        _ => None,
    };

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactIn,
        input_token: asset_with_amount(
            v4_asset_ref(ctx, &currency_in),
            amount_constraint(AmountKind::Exact, decimal(&amount_in.to_string())?),
        ),
        output_token: asset_with_amount(
            v4_asset_ref(ctx, &currency_out),
            amount_constraint(AmountKind::Min, decimal(&amount_out_min.to_string())?),
        ),
        recipient: ctx.from.clone(),
        validity,
        fee_bps,
    }))
}
