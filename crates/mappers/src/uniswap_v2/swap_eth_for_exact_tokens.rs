//! V2 Router02 `swapETHForExactTokens` (payable) — selector 0xfb3bdb41.
//! `amountInMax` = msg.value, `amountOut` from calldata. Refunds excess ETH.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v2::common::{
    deadline_from, envelope_swap, token_in_out, tx_value_u256, ET, V2_FEE_BPS,
};

pub const SELECTOR: [u8; 4] = [0xfb, 0x3b, 0xdb, 0x41];

pub fn map(
    ctx: &BuildContext,
    tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    let amount_out = call.arg("amountOut")?.as_uint()?;
    let path = call.arg("path")?.as_address_array()?;
    let to = call.arg("to")?.as_address()?;
    let deadline = call.arg("deadline")?.as_uint()?;

    let (token_in, token_out) = token_in_out(ctx, &path, ET)?;
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactOut,
        token_in,
        token_out,
        amount_in: AmountConstraint::max(tx_value_u256(tx).to_string()),
        amount_out: AmountConstraint::exact(amount_out.to_string()),
        recipient: Some(addr_to_string(to)),
        deadline_seconds_from_now: deadline_from(deadline, ctx),
        fee_bps: Some(V2_FEE_BPS),
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
