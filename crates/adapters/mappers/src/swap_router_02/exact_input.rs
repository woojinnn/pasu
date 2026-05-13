//! SwapRouter02 exactInput — selector 0xb858183f.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v3::common::{envelope_swap, fee_tier_to_bps, parse_path};

pub const SELECTOR: [u8; 4] = [0xb8, 0x58, 0x18, 0x3f];

pub fn map(
    ctx: &BuildContext,
    _tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    let p = call.arg("params")?;
    let path = p.field("path")?.as_bytes()?;
    let recipient = p.field("recipient")?.as_address()?;
    let amount_in = p.field("amountIn")?.as_uint()?;
    let amount_out_min = p.field("amountOutMinimum")?.as_uint()?;

    let (tokens, fees) = parse_path(path)?;
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
