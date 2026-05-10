//! Uniswap V4 swap decoder.
//!
//! V4는 일반적으로 Universal Router opcode `0x10 V4_SWAP`을 통해 진입.
//! 이 모듈은 V4 swap action params에서 `PoolKey` + 입출력 토큰을 추출해
//! `SwapFields`를 빌드.
//!
//! # V4 SWAP_EXACT_IN_SINGLE 형태 (UR `V4_SWAP` 안의 action `0x06`):
//! ```text
//! ((PoolKey, bool zeroForOne, uint256 amountSpecified, uint160 sqrtPriceLimitX96, bytes hookData))
//! ```
//!
//! `PoolKey = (currency0, currency1, fee:uint24, tickSpacing:int24, hooks)`.
//! `fee`의 최상위 bit `0x800000`은 dynamic-fee marker — 마스킹 후 사용.

use serde_json::Value;

use crate::action::fields::{
    HopRef, SettlementKind, SlippageInfo, SlippageSource, SwapFields, SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_min, recipients_from, swap_hook_flags,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{Address, AmountSpec, DeadlineFields};

const DYNAMIC_FEE_MASK: u32 = 0x800000;

/// V4 fee uint24 → bps. dynamic-fee marker 마스킹 후 1/100 적용.
pub fn v4_fee_to_bps(fee: u32) -> u32 {
    (fee & !DYNAMIC_FEE_MASK) / 100
}

/// args의 `poolKey` 객체에서 currency0/1, fee, tickSpacing, hooks 추출.
pub fn parse_pool_key(pk: &Value) -> Result<(Address, Address, u32, i32, Address), SemiAdapterError> {
    let currency0: Address = pk
        .get("currency0")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "poolKey.currency0" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "poolKey.currency0".into(),
        })?;
    let currency1: Address = pk
        .get("currency1")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "poolKey.currency1" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "poolKey.currency1".into(),
        })?;
    let fee = pk
        .get("fee")
        .and_then(|v| v.as_u64())
        .ok_or(SemiAdapterError::MissingArg { name: "poolKey.fee" })? as u32;
    let tick_spacing = pk
        .get("tickSpacing")
        .and_then(|v| v.as_i64())
        .ok_or(SemiAdapterError::MissingArg { name: "poolKey.tickSpacing" })? as i32;
    let hooks: Address = pk
        .get("hooks")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "poolKey.hooks" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "poolKey.hooks".into(),
        })?;
    Ok((currency0, currency1, fee, tick_spacing, hooks))
}

/// V4 swap params (보통 `actions: 0x06 SWAP_EXACT_IN_SINGLE`)에서 SwapFields 빌드.
///
/// 단순화 — input.json의 args에 `poolKey` 객체 + `zeroForOne` + `amountSpecified` + `amountOutMin`이 있다고 가정.
pub fn build_v4_swap_fields(args: &Value, ctx: &BuildContext) -> Result<SwapFields, SemiAdapterError> {
    let pk = args
        .get("poolKey")
        .ok_or(SemiAdapterError::MissingArg { name: "poolKey" })?;
    let (c0, c1, fee, _tick, hooks) = parse_pool_key(pk)?;

    let zero_for_one = args
        .get("zeroForOne")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let (token_in_addr, token_out_addr) = if zero_for_one { (c0, c1) } else { (c1, c0) };

    let amount_in_raw = args
        .get("amountIn")
        .or_else(|| args.get("amountSpecified"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| ctx.value_wei.clone());

    let amount_out_min_raw = args
        .get("amountOutMin")
        .or_else(|| args.get("amountOutMinimum"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "0".into());

    let token_in = token_metadata(token_in_addr, ctx.chain_id);
    let token_out = token_metadata(token_out_addr, ctx.chain_id);

    let fee_bps = v4_fee_to_bps(fee);

    let hook_active = hooks != Address::ZERO;
    let confidence = if hook_active { Confidence::Medium } else { Confidence::High };
    let _hook_flags = swap_hook_flags(hooks);

    let hop = HopRef {
        id: "h#0".into(),
        protocol: "uniswap.v4".into(),
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        pool: None,
        fee_bps: Some(fee_bps),
        confidence,
    };

    let amount_out = amount_min(amount_out_min_raw.clone());
    let has_zero_min_output = amount_out_min_raw == "0";

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["uniswap.v4".into()],
        input_tokens: vec![token_in],
        output_tokens: vec![token_out],
        mode: SwapMode::ExactIn,
        amount_in: amount_exact(amount_in_raw),
        amount_out,
        route: SwapRoute::SingleHop { hop },
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: Some(AmountSpec {
                raw: amount_out_min_raw,
                kind: crate::types::AmountKind::Min,
            }),
        },
        settlement: SettlementKind::Callback,
        recipients: recipients_from(None, ctx.actor),
        deadlines: DeadlineFields {
            deadline: None,
            deadline_horizon_seconds: ctx.block_timestamp.and(None),
        },
        max_fee_bps: Some(fee_bps),
        has_zero_min_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_bps_strips_dynamic_marker() {
        assert_eq!(v4_fee_to_bps(500), 5);
        assert_eq!(v4_fee_to_bps(0x800000 | 500), 5);
    }

    #[test]
    fn pool_key_parse() {
        let pk = serde_json::json!({
            "currency0": "0x0000000000000000000000000000000000000000",
            "currency1": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "fee": 100,
            "tickSpacing": 1,
            "hooks": "0x0000000000000000000000000000000000000000"
        });
        let (c0, c1, fee, ts, h) = parse_pool_key(&pk).unwrap();
        assert_eq!(fee, 100);
        assert_eq!(ts, 1);
        assert!(c0 < c1);
        assert_eq!(h, Address::ZERO);
    }
}
