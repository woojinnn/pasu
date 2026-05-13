//! SwapRouter02 exactOutputSingle — selector 0x5023b4df.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v3::common::{envelope_swap, fee_tier_to_bps};

pub const SELECTOR: [u8; 4] = [0x50, 0x23, 0xb4, 0xdf];

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
    let amount_out = p.field("amountOut")?.as_uint()?;
    let amount_in_max = p.field("amountInMaximum")?.as_uint()?;

    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: ctx.tokens.erc20(ctx.chain_id, token_in),
        token_out: ctx.tokens.erc20(ctx.chain_id, token_out),
        amount_in: AmountConstraint::max(amount_in_max.to_string()),
        amount_out: AmountConstraint::exact(amount_out.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: None,
        fee_bps: Some(fee_tier_to_bps(fee)),
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
