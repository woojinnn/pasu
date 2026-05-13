//! UR command 0x00 V3_SWAP_EXACT_IN.
//!
//! Input (latest):
//!   (address recipient, uint256 amountIn, uint256 amountOutMinimum,
//!    bytes path, bool payerIsUser, uint256[] minHopPriceX36)

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
    let (recipient, amount_in, amount_out_min, path_b) =
        if let Ok((r, ai, ao, p, _, _)) = ArgsLatest::abi_decode_sequence(input, true) {
            (r, ai, ao, p)
        } else {
            let (r, ai, ao, p, _) = ArgsOld::abi_decode_sequence(input, true)
                .map_err(|e| MapError::AbiDecode(e.to_string()))?;
            (r, ai, ao, p)
        };
    let (tokens, fees) = parse_path(&path_b)?;
    let fee_bps = if fees.len() == 1 {
        Some(fee_tier_to_bps(fees[0]))
    } else {
        None
    };
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: ctx.tokens.erc20(ctx.chain_id, *tokens.first().unwrap()),
        token_out: ctx.tokens.erc20(ctx.chain_id, *tokens.last().unwrap()),
        amount_in: AmountConstraint::exact(amount_in.to_string()),
        amount_out: AmountConstraint::min(amount_out_min.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: None,
        fee_bps,
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
