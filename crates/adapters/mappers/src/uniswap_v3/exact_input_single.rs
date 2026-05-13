//! V3 SwapRouter `exactInputSingle(ExactInputSingleParams)` — selector 0x414bf389.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v3::common::{deadline_from, envelope_swap, fee_tier_to_bps};

pub const SELECTOR: [u8; 4] = [0x41, 0x4b, 0xf3, 0x89];

pub fn map(
    ctx: &BuildContext,
    _tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    let p = call.arg("params")?;
    let token_in = p.field("tokenIn")?.as_address()?;
    let token_out = p.field("tokenOut")?.as_address()?;
    let fee = p.field("fee")?.as_uint()?.to::<u32>();
    let recipient = p.field("recipient")?.as_address()?;
    let deadline = p.field("deadline")?.as_uint()?;
    let amount_in = p.field("amountIn")?.as_uint()?;
    let amount_out_min = p.field("amountOutMinimum")?.as_uint()?;

    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: ctx.tokens.erc20(ctx.chain_id, token_in),
        token_out: ctx.tokens.erc20(ctx.chain_id, token_out),
        amount_in: AmountConstraint::exact(amount_in.to_string()),
        amount_out: AmountConstraint::min(amount_out_min.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: deadline_from(deadline, ctx),
        fee_bps: Some(fee_tier_to_bps(fee)),
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
