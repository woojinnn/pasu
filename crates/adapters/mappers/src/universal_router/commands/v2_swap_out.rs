//! UR command 0x09 V2_SWAP_EXACT_OUT.

use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::SolValue;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v2::common::{envelope_swap, token_in_out, TT, V2_FEE_BPS};

type ArgsLatest = (AlloyAddress, U256, U256, Vec<AlloyAddress>, bool, Vec<U256>);
type ArgsOld = (AlloyAddress, U256, U256, Vec<AlloyAddress>, bool);

pub fn map_command(
    ctx: &BuildContext,
    _tx: &RawTx,
    input: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    // For V2_SWAP_EXACT_OUT: (recipient, amountOut, amountInMax, path, payerIsUser, [minHopPriceX36])
    let (recipient, amount_out, amount_in_max, path) =
        if let Ok((r, ao, ai, p, _, _)) = ArgsLatest::abi_decode_sequence(input, true) {
            (r, ao, ai, p)
        } else {
            let (r, ao, ai, p, _) = ArgsOld::abi_decode_sequence(input, true)
                .map_err(|e| MapError::AbiDecode(e.to_string()))?;
            (r, ao, ai, p)
        };
    let (token_in, token_out) = token_in_out(ctx, &path, TT)?;
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactOut,
        token_in,
        token_out,
        amount_in: AmountConstraint::max(amount_in_max.to_string()),
        amount_out: AmountConstraint::exact(amount_out.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: None,
        fee_bps: Some(V2_FEE_BPS),
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
