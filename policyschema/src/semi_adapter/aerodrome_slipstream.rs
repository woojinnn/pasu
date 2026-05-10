//! Aerodrome Slipstream (Aerodrome의 Uniswap V3 fork) decoder.
//!
//! V3와 거의 동일하나 `feeTier` 대신 `tickSpacing`(int24) 사용.

use serde_json::Value;

use crate::action::fields::{
    HopRef, SettlementKind, SlippageInfo, SlippageSource, SwapFields, SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_min, as_address, as_u64, as_uint_string, deadline_horizon, recipients_from,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::DeadlineFields;

/// Slipstream `exactInputSingle` (V3와 동일 selector).
pub const SEL_EXACT_INPUT_SINGLE: [u8; 4] = [0x04, 0xe4, 0x5a, 0xaf];

/// `tickSpacing` → 추정 fee bps. 어댑터 정적 매핑.
pub fn tick_spacing_to_fee_bps(tick_spacing: i32) -> u32 {
    match tick_spacing {
        1 => 1,    // 0.01%
        50 => 5,   // 0.05%
        100 => 30, // 0.3%
        200 => 100, // 1%
        _ => 30,   // fallback
    }
}

pub fn build_slipstream_swap_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let token_in_addr = as_address(args, "tokenIn")?;
    let token_out_addr = as_address(args, "tokenOut")?;
    let tick_spacing = args
        .get("tickSpacing")
        .and_then(|v| v.as_i64())
        .ok_or(SemiAdapterError::MissingArg { name: "tickSpacing" })? as i32;
    let recipient = as_address(args, "recipient")?;
    let deadline = as_u64(args, "deadline").ok();
    let amount_in = as_uint_string(args, "amountIn")?;
    let amount_out_min = as_uint_string(args, "amountOutMinimum")?;

    let token_in = token_metadata(token_in_addr, ctx.chain_id);
    let token_out = token_metadata(token_out_addr, ctx.chain_id);
    let fee_bps = tick_spacing_to_fee_bps(tick_spacing);

    let hop = HopRef {
        id: "h#0".into(),
        protocol: "aerodrome.slipstream".into(),
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        pool: None,
        fee_bps: Some(fee_bps),
        confidence: Confidence::High,
    };

    let amount_out = amount_min(amount_out_min);
    let has_zero_min_output = amount_out.raw == "0";

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["aerodrome.slipstream".into()],
        input_tokens: vec![token_in],
        output_tokens: vec![token_out],
        mode: SwapMode::ExactIn,
        amount_in: amount_exact(amount_in),
        amount_out: amount_out.clone(),
        route: SwapRoute::SingleHop { hop },
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: Some(amount_out),
        },
        settlement: SettlementKind::Callback,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, ctx.block_timestamp)),
        },
        max_fee_bps: Some(fee_bps),
        has_zero_min_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tick_spacing_mapping() {
        assert_eq!(tick_spacing_to_fee_bps(50), 5);
        assert_eq!(tick_spacing_to_fee_bps(100), 30);
        assert_eq!(tick_spacing_to_fee_bps(200), 100);
    }
}
