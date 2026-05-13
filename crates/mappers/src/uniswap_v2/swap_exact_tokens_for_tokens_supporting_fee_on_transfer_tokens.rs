//! V2 Router02 `swapExactTokensForTokensSupportingFeeOnTransferTokens` — selector 0x5c11d795.
//! Same calldata shape as `swapExactTokensForTokens`.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v2::common::{deadline_from, envelope_swap, token_in_out, TT, V2_FEE_BPS};

pub const SELECTOR: [u8; 4] = [0x5c, 0x11, 0xd7, 0x95];

pub fn map(
    ctx: &BuildContext,
    _tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    let amount_in = call.arg("amountIn")?.as_uint()?;
    let amount_out_min = call.arg("amountOutMin")?.as_uint()?;
    let path = call.arg("path")?.as_address_array()?;
    let to = call.arg("to")?.as_address()?;
    let deadline = call.arg("deadline")?.as_uint()?;

    let (token_in, token_out) = token_in_out(ctx, &path, TT)?;
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactIn,
        token_in,
        token_out,
        amount_in: AmountConstraint::exact(amount_in.to_string()),
        amount_out: AmountConstraint::min(amount_out_min.to_string()),
        recipient: Some(addr_to_string(to)),
        deadline_seconds_from_now: deadline_from(deadline, ctx),
        fee_bps: Some(V2_FEE_BPS),
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
