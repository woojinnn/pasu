//! `V4Router.swapExactInSingle(params: ExactInputSingleParams)`.
//! params: (poolKey, zeroForOne, amountIn, amountOutMinimum, minHopPriceX36, hookData)

use abi_resolver::subdecode::opcode_stream::DecodedStep;
use alloy_dyn_abi::DynSolValue;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind, Validity};

use super::super::common::{
    amount_constraint, asset_with_amount, decimal, swap_envelope, v4_asset_ref,
};
use super::{extract_pool_fee_bps, tuple_address, tuple_bool, tuple_uint, v4_params_tuple};
use crate::{AdapterError, CallContext};

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    step: &DecodedStep,
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let fields = v4_params_tuple(step)?;
    let pool_key = fields
        .first()
        .ok_or_else(|| AdapterError::Invalid("V4 ExactInSingle missing poolKey".into()))?;
    let DynSolValue::Tuple(pk) = pool_key else {
        return Err(AdapterError::Invalid("V4 poolKey not a tuple".into()));
    };
    let currency0 = tuple_address(&pk[0], "poolKey.currency0")?;
    let currency1 = tuple_address(&pk[1], "poolKey.currency1")?;
    let zero_for_one = tuple_bool(&fields[1], "zeroForOne")?;
    let amount_in = tuple_uint(&fields[2], "amountIn")?;
    let amount_out_min = tuple_uint(&fields[3], "amountOutMinimum")?;

    let (token_in, token_out) = if zero_for_one {
        (currency0, currency1)
    } else {
        (currency1, currency0)
    };

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactIn,
        input_token: asset_with_amount(
            v4_asset_ref(ctx, &token_in),
            amount_constraint(AmountKind::Exact, decimal(&amount_in.to_string())?),
        ),
        output_token: asset_with_amount(
            v4_asset_ref(ctx, &token_out),
            amount_constraint(AmountKind::Min, decimal(&amount_out_min.to_string())?),
        ),
        // V4 doesn't carry recipient in swap params (uses delta + take action)
        recipient: ctx.from.clone(),
        validity,
        fee_bps: extract_pool_fee_bps(&pk)?,
    }))
}
