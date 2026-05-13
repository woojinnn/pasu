//! V3 SwapRouter `exactOutput(ExactOutputParams)` — selector 0xf28c0498.
//! `path` here is REVERSED: tokenOut | fee | … | tokenIn.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v3::common::{deadline_from, envelope_swap, fee_tier_to_bps, parse_path};

pub const SELECTOR: [u8; 4] = [0xf2, 0x8c, 0x04, 0x98];

pub fn map(
    ctx: &BuildContext,
    _tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    let p = call.arg("params")?;
    let path = p.field("path")?.as_bytes()?;
    let recipient = p.field("recipient")?.as_address()?;
    let deadline = p.field("deadline")?.as_uint()?;
    let amount_out = p.field("amountOut")?.as_uint()?;
    let amount_in_max = p.field("amountInMaximum")?.as_uint()?;

    // path reversed: first=tokenOut, last=tokenIn
    let (tokens, fees) = parse_path(path)?;
    let token_out_addr = *tokens.first().unwrap();
    let token_in_addr = *tokens.last().unwrap();
    let fee_bps = if fees.len() == 1 {
        Some(fee_tier_to_bps(fees[0]))
    } else {
        None
    };
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: ctx.tokens.erc20(ctx.chain_id, token_in_addr),
        token_out: ctx.tokens.erc20(ctx.chain_id, token_out_addr),
        amount_in: AmountConstraint::max(amount_in_max.to_string()),
        amount_out: AmountConstraint::exact(amount_out.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: deadline_from(deadline, ctx),
        fee_bps,
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
