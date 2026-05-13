//! UR command 0x01 V3_SWAP_EXACT_OUT — same shape as V3_SWAP_EXACT_IN with
//! `amountInMax`/`amountOut`. `path` is REVERSED (out → in).

use alloy_primitives::{Address as AlloyAddress, Bytes, U256};
use alloy_sol_types::SolValue;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v3::common::{envelope_swap, fee_tier_to_bps, parse_path};

type ArgsLatest = (AlloyAddress, U256, U256, Bytes, bool, Vec<U256>);
type ArgsOld = (AlloyAddress, U256, U256, Bytes, bool);

pub fn map_command(
    ctx: &BuildContext,
    _tx: &RawTx,
    input: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    let (recipient, amount_out, amount_in_max, path_b) =
        if let Ok((r, ao, ai, p, _, _)) = ArgsLatest::abi_decode_sequence(input, true) {
            (r, ao, ai, p)
        } else {
            let (r, ao, ai, p, _) = ArgsOld::abi_decode_sequence(input, true)
                .map_err(|e| MapError::AbiDecode(e.to_string()))?;
            (r, ao, ai, p)
        };
    let (tokens, fees) = parse_path(&path_b)?;
    let fee_bps = if fees.len() == 1 {
        Some(fee_tier_to_bps(fees[0]))
    } else {
        None
    };
    // Reversed: first = tokenOut, last = tokenIn
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: ctx.tokens.erc20(ctx.chain_id, *tokens.last().unwrap()),
        token_out: ctx.tokens.erc20(ctx.chain_id, *tokens.first().unwrap()),
        amount_in: AmountConstraint::max(amount_in_max.to_string()),
        amount_out: AmountConstraint::exact(amount_out.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: None,
        fee_bps,
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
