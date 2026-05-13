//! V3 SwapRouter `exactInput(ExactInputParams)` — selector 0xc04b8d59.
//! Encoded `path` = tokenIn | fee | … | tokenOut.
//!
//! Migrated to consume `abi_resolver::decode::DecodedCall` instead of an
//! inline `sol!` decode. The decoded call is produced by abi-resolver's
//! Sourcify-backed Resolver and reaches us via `registry::dispatch`.

use abi_resolver::decode::DecodedCall;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v3::common::{deadline_from, envelope_swap, fee_tier_to_bps, parse_path};

/// `exactInput((bytes,address,uint256,uint256,uint256))` selector. Kept as a
/// const so `registry::dispatch` can pattern-match selectors without parsing
/// the signature string at every call.
pub const SELECTOR: [u8; 4] = [0xc0, 0x4b, 0x8d, 0x59];

pub fn map(
    ctx: &BuildContext,
    _tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    let params = call.arg("params")?;
    let path = params.field("path")?.as_bytes()?;
    let recipient = params.field("recipient")?.as_address()?;
    let deadline = params.field("deadline")?.as_uint()?;
    let amount_in = params.field("amountIn")?.as_uint()?;
    let amount_out_min = params.field("amountOutMinimum")?.as_uint()?;

    let (tokens, fees) = parse_path(path)?;
    let token_in_addr = *tokens.first().unwrap();
    let token_out_addr = *tokens.last().unwrap();
    let fee_bps = if fees.len() == 1 {
        Some(fee_tier_to_bps(fees[0]))
    } else {
        None
    };
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: ctx.tokens.erc20(ctx.chain_id, token_in_addr),
        token_out: ctx.tokens.erc20(ctx.chain_id, token_out_addr),
        amount_in: AmountConstraint::exact(amount_in.to_string()),
        amount_out: AmountConstraint::min(amount_out_min.to_string()),
        recipient: Some(addr_to_string(recipient)),
        deadline_seconds_from_now: deadline_from(deadline, ctx),
        fee_bps,
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
