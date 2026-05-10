//! Uniswap V3 SwapRouter / SwapRouter02 decoder.
//!
//! 4개 swap 함수 + V3 encoded path 분해.
//!
//! | selector | 시그니처 | mode | route |
//! |---|---|---|---|
//! | 0x04e45aaf | exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160)) | ExactIn | SingleHop |
//! | 0xb858183f | exactInput((bytes,address,uint256,uint256,uint256)) | ExactIn | MultiHop |
//! | 0x5023b4df | exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160)) | ExactOut | SingleHop |
//! | 0x09b81346 | exactOutput((bytes,address,uint256,uint256,uint256)) | ExactOut | MultiHop |
//!
//! V3 fee tier (uint24)는 1/100 bps 단위. 예: 500 = 5 bps = 0.05%.

use serde_json::Value;

use crate::action::fields::{
    HopRef, SettlementKind, SlippageInfo, SlippageSource, SwapFields, SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_max, amount_min, as_address, as_u64, as_uint_string, deadline_horizon,
    recipients_from, v3_path_to_hops,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::DeadlineFields;
#[cfg(test)]
use crate::types::Address;

pub const SEL_EXACT_INPUT_SINGLE: [u8; 4] = [0x04, 0xe4, 0x5a, 0xaf];
pub const SEL_EXACT_INPUT: [u8; 4] = [0xb8, 0x58, 0x18, 0x3f];
pub const SEL_EXACT_OUTPUT_SINGLE: [u8; 4] = [0x50, 0x23, 0xb4, 0xdf];
pub const SEL_EXACT_OUTPUT: [u8; 4] = [0x09, 0xb8, 0x13, 0x46];

/// V3 fee tier(uint24, 1/100 bps) → bps.
fn fee_tier_to_bps(fee_tier: u32) -> u32 {
    fee_tier / 100
}

/// `exactInputSingle` / `exactOutputSingle` — struct args.
fn decode_single(
    args: &Value,
    ctx: &BuildContext,
    mode: SwapMode,
) -> Result<SwapFields, SemiAdapterError> {
    let token_in_addr = as_address(args, "tokenIn")?;
    let token_out_addr = as_address(args, "tokenOut")?;
    let fee_tier = as_u64(args, "fee")? as u32;
    let recipient = as_address(args, "recipient")?;
    let deadline = as_u64(args, "deadline").ok();

    let (amount_in, amount_out) = match mode {
        SwapMode::ExactIn => (
            amount_exact(as_uint_string(args, "amountIn")?),
            amount_min(as_uint_string(args, "amountOutMinimum")?),
        ),
        SwapMode::ExactOut => (
            amount_max(as_uint_string(args, "amountInMaximum")?),
            amount_exact(as_uint_string(args, "amountOut")?),
        ),
        _ => {
            return Err(SemiAdapterError::AbiDecode {
                reason: "V3 single hop은 ExactIn/ExactOut만".into(),
            })
        }
    };

    let token_in = token_metadata(token_in_addr, ctx.chain_id);
    let token_out = token_metadata(token_out_addr, ctx.chain_id);

    let hop = HopRef {
        id: "h#0".into(),
        protocol: "uniswap.v3".into(),
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        pool: None,
        fee_bps: Some(fee_tier_to_bps(fee_tier)),
        confidence: Confidence::High,
    };

    let amount_out_min_for_slippage = if matches!(mode, SwapMode::ExactIn) {
        Some(amount_out.clone())
    } else {
        None
    };

    let has_zero_min_output = matches!(mode, SwapMode::ExactIn) && amount_out.raw == "0";

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["uniswap.v3".into()],
        input_tokens: vec![token_in],
        output_tokens: vec![token_out],
        mode,
        amount_in,
        amount_out,
        route: SwapRoute::SingleHop { hop },
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: amount_out_min_for_slippage,
        },
        settlement: SettlementKind::Callback,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, ctx.block_timestamp)),
        },
        max_fee_bps: Some(fee_tier_to_bps(fee_tier)),
        has_zero_min_output,
    })
}

/// `exactInput` / `exactOutput` — encoded path 사용.
fn decode_multi(
    args: &Value,
    ctx: &BuildContext,
    mode: SwapMode,
) -> Result<SwapFields, SemiAdapterError> {
    let path_hex = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "path" })?;
    let path_bytes = hex::decode(path_hex.trim_start_matches("0x"))
        .map_err(|e| SemiAdapterError::BadHex(e.to_string()))?;

    let recipient = as_address(args, "recipient")?;
    let deadline = as_u64(args, "deadline").ok();

    let (amount_in, amount_out) = match mode {
        SwapMode::ExactIn => (
            amount_exact(as_uint_string(args, "amountIn")?),
            amount_min(as_uint_string(args, "amountOutMinimum")?),
        ),
        SwapMode::ExactOut => (
            amount_max(as_uint_string(args, "amountInMaximum")?),
            amount_exact(as_uint_string(args, "amountOut")?),
        ),
        _ => {
            return Err(SemiAdapterError::AbiDecode {
                reason: "V3 multi hop은 ExactIn/ExactOut만".into(),
            })
        }
    };

    // exactOutput는 path가 역순으로 인코딩됨
    let reverse = matches!(mode, SwapMode::ExactOut);
    let v3_hops = v3_path_to_hops(&path_bytes, ctx.chain_id, reverse)?;

    if v3_hops.is_empty() {
        return Err(SemiAdapterError::BadV3Path {
            length: path_bytes.len(),
        });
    }

    // 첫·끝 hop의 토큰 → input/output
    let input_token = v3_hops.first().unwrap().token_in.clone();
    let output_token = v3_hops.last().unwrap().token_out.clone();

    let hops: Vec<HopRef> = v3_hops
        .iter()
        .enumerate()
        .map(|(i, h)| HopRef {
            id: format!("h#{i}"),
            protocol: "uniswap.v3".into(),
            token_in: h.token_in.clone(),
            token_out: h.token_out.clone(),
            pool: None,
            fee_bps: Some(fee_tier_to_bps(h.fee_tier)),
            confidence: Confidence::High,
        })
        .collect();

    let max_fee_bps = hops
        .iter()
        .filter_map(|h| h.fee_bps)
        .max();

    let route = if hops.len() == 1 {
        SwapRoute::SingleHop {
            hop: hops.into_iter().next().unwrap(),
        }
    } else {
        SwapRoute::MultiHop { hops }
    };

    let amount_out_min_for_slippage = if matches!(mode, SwapMode::ExactIn) {
        Some(amount_out.clone())
    } else {
        None
    };

    let has_zero_min_output = matches!(mode, SwapMode::ExactIn) && amount_out.raw == "0";

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["uniswap.v3".into()],
        input_tokens: vec![input_token],
        output_tokens: vec![output_token],
        mode,
        amount_in,
        amount_out,
        route,
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: amount_out_min_for_slippage,
        },
        settlement: SettlementKind::Callback,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, ctx.block_timestamp)),
        },
        max_fee_bps,
        has_zero_min_output,
    })
}

/// 셀렉터·args·ctx → V3 SwapFields.
pub fn build_v3_swap_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    match *selector {
        SEL_EXACT_INPUT_SINGLE => decode_single(args, ctx, SwapMode::ExactIn),
        SEL_EXACT_OUTPUT_SINGLE => decode_single(args, ctx, SwapMode::ExactOut),
        SEL_EXACT_INPUT => decode_multi(args, ctx, SwapMode::ExactIn),
        SEL_EXACT_OUTPUT => decode_multi(args, ctx, SwapMode::ExactOut),
        _ => Err(SemiAdapterError::BadSelector {
            expected: "Uniswap V3 swap selector".into(),
            got: format!("0x{}", hex::encode(selector)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::Confidence;
    use crate::target::{ContractTarget, DiscoveredBy, TargetRole, Verification};
    use serde_json::json;

    fn ctx(actor: Address) -> (Vec<ContractTarget>, BuildContext<'static>) {
        let target: Address = "0xE592427A0AEce92De3Edee1F18E0157C05861564"
            .parse()
            .unwrap();
        let targets: Vec<ContractTarget> = vec![ContractTarget {
            id: "t#router".into(),
            address: target,
            chain_id: 1,
            role: TargetRole::Router,
            protocol: None,
            discovered_by: DiscoveredBy::TxTo,
            verification: Verification {
                label_source: "curated".into(),
                abi_available: true,
                contract_verified: true,
                proxy_resolved: None,
            },
            confidence: Confidence::High,
        }];
        let leaked: &'static [ContractTarget] = Box::leak(targets.clone().into_boxed_slice());
        (
            targets,
            BuildContext {
                chain_id: 1,
                actor,
                target,
                value_wei: "0".into(),
                block_timestamp: Some(1_762_499_000),
                targets: leaked,
            },
        )
    }

    #[test]
    fn exact_input_single() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let (_t, ctx) = ctx(actor);
        let args = json!({
            "tokenIn": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "tokenOut": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "fee": 500,
            "recipient": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000",
            "amountIn": "1000000000",
            "amountOutMinimum": "300000000000000000",
            "sqrtPriceLimitX96": "0"
        });
        let fields = build_v3_swap_fields(&SEL_EXACT_INPUT_SINGLE, &args, &ctx).unwrap();
        assert_eq!(fields.mode, SwapMode::ExactIn);
        assert_eq!(fields.max_fee_bps, Some(5)); // 500/100
        assert!(matches!(fields.route, SwapRoute::SingleHop { .. }));
    }

    #[test]
    fn exact_output_single_swaps_amount_kinds() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let (_t, ctx) = ctx(actor);
        let args = json!({
            "tokenIn": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "tokenOut": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "fee": 500,
            "recipient": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000",
            "amountOut": "1000000000",
            "amountInMaximum": "400000000000000000",
            "sqrtPriceLimitX96": "0"
        });
        let fields = build_v3_swap_fields(&SEL_EXACT_OUTPUT_SINGLE, &args, &ctx).unwrap();
        assert_eq!(fields.mode, SwapMode::ExactOut);
        use crate::types::AmountKind;
        assert_eq!(fields.amount_in.kind, AmountKind::Max);
        assert_eq!(fields.amount_out.kind, AmountKind::Exact);
        assert!(!fields.has_zero_min_output);
    }

    #[test]
    fn exact_input_multi_path_decode() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let (_t, ctx) = ctx(actor);
        // USDC + 500 + WETH (single hop encoded path)
        let path = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB480001f4C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let args = json!({
            "path": path,
            "recipient": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000",
            "amountIn": "1000000000",
            "amountOutMinimum": "300000000000000000"
        });
        let fields = build_v3_swap_fields(&SEL_EXACT_INPUT, &args, &ctx).unwrap();
        assert_eq!(fields.mode, SwapMode::ExactIn);
        assert_eq!(fields.input_tokens[0].symbol, "USDC");
        assert_eq!(fields.output_tokens[0].symbol, "WETH");
        assert_eq!(fields.max_fee_bps, Some(5));
    }
}
