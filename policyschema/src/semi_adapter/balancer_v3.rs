//! Balancer V3 — Vault 재설계 + BatchRouter / CompositeRouter 별도 진입점.
//!
//! V2와의 차이:
//! - `bytes32 poolId` → `address pool` 직접 사용
//! - Hook 시스템 (Uniswap V4 영향)
//! - batchSwap이 Vault가 아닌 **BatchRouter** 컨트랙트로 이동 ⭐
//! - Custom AMM, Boosted Pool 등 새 풀 타입
//!
//! 5 핵심 함수:
//! - V3 Vault `swap(VaultSwapParams)` — single
//! - V3 Vault `addLiquidity(AddLiquidityParams)` — LP 추가
//! - V3 Vault `removeLiquidity(RemoveLiquidityParams)` — LP 회수
//! - V3 BatchRouter `swapExactIn(SwapPathExactAmountIn[], deadline, wethIsEth, userData)` — **멀티홉** ⭐
//! - V3 BatchRouter `swapExactOut(SwapPathExactAmountOut[], ...)` — **멀티홉** ⭐

use serde_json::Value;

use crate::action::fields::{
    HopRef, LiquidityFields, SettlementKind, SlippageInfo, SlippageSource, SwapFields, SwapMode,
    SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_max, amount_min, as_u64, deadline_horizon, recipients_from,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{Address, AmountKind, AmountSpec, DeadlineFields, Token};

pub const VAULT_MAINNET_HEX: &str = "0xbA1333333333a1BA1108E8412f11850A5C319bA9";
pub const BATCH_ROUTER_MAINNET_HEX: &str = "0x136f1efcc3f8f88516b9e94110d56fdbfb1778d1";

// V3 Vault selectors (placeholder — 실제 검증 필요)
pub const SEL_V3_VAULT_SWAP: [u8; 4] = [0x2b, 0xfa, 0xa4, 0x59];
pub const SEL_V3_VAULT_ADD_LIQUIDITY: [u8; 4] = [0x55, 0x49, 0xa3, 0xb0];
pub const SEL_V3_VAULT_REMOVE_LIQUIDITY: [u8; 4] = [0xab, 0x55, 0x49, 0xa3];

// V3 BatchRouter selectors
pub const SEL_V3_BATCH_SWAP_EXACT_IN: [u8; 4] = [0x28, 0x6f, 0x58, 0x0d];
pub const SEL_V3_BATCH_SWAP_EXACT_OUT: [u8; 4] = [0x9a, 0x99, 0xb4, 0xf0];

// ===========================================================================
// V3 Vault single swap
// ===========================================================================

/// V3 Vault `swap(VaultSwapParams)`.
///
/// `VaultSwapParams = { kind: SwapKind, pool: address, tokenIn: IERC20, tokenOut: IERC20, amountGivenRaw: uint256, limitRaw: uint256, userData: bytes }`.
pub fn build_balancer_v3_swap_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let params = args.get("params").unwrap_or(args);

    let kind_n = params.get("kind").and_then(|v| v.as_u64()).unwrap_or(0);
    let mode = if kind_n == 0 { SwapMode::ExactIn } else { SwapMode::ExactOut };

    let pool_addr = parse_addr_field(params.get("pool"), "params.pool")?;
    let token_in_addr = parse_addr_field(params.get("tokenIn"), "params.tokenIn")?;
    let token_out_addr = parse_addr_field(params.get("tokenOut"), "params.tokenOut")?;

    let amount_given = params
        .get("amountGivenRaw")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or(SemiAdapterError::MissingArg { name: "params.amountGivenRaw" })?;
    let limit = params
        .get("limitRaw")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or(SemiAdapterError::MissingArg { name: "params.limitRaw" })?;

    let deadline = args.get("deadline").and_then(|v| v.as_str()).and_then(|s| s.parse().ok());

    let token_in = token_metadata(token_in_addr, ctx.chain_id);
    let token_out = token_metadata(token_out_addr, ctx.chain_id);

    let (amount_in, amount_out) = match mode {
        SwapMode::ExactIn => (amount_exact(amount_given), amount_min(limit)),
        _ => (amount_max(limit), amount_exact(amount_given)),
    };

    let hop = HopRef {
        id: "h#0".into(),
        protocol: "balancer.v3".into(),
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        pool: Some(format!("t#pool-{pool_addr:#x}")),
        fee_bps: None,
        confidence: Confidence::High,
    };

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v3".into()],
        input_tokens: vec![token_in],
        output_tokens: vec![token_out],
        mode,
        amount_in,
        amount_out: amount_out.clone(),
        route: SwapRoute::SingleHop { hop },
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: matches!(mode, SwapMode::ExactIn).then(|| amount_out.clone()),
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(None, ctx.actor),
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, ctx.block_timestamp)),
        },
        max_fee_bps: None,
        has_zero_min_output: matches!(mode, SwapMode::ExactIn) && amount_out.raw == "0",
    })
}

// ===========================================================================
// V3 Vault addLiquidity / removeLiquidity
// ===========================================================================

pub fn build_balancer_v3_add_liquidity_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LiquidityFields, SemiAdapterError> {
    let params = args.get("params").unwrap_or(args);
    let pool_addr = parse_addr_field(params.get("pool"), "params.pool")?;
    let tokens_arr = params
        .get("tokensIn")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "params.tokensIn" })?;
    let max_amounts = params
        .get("maxAmountsIn")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "params.maxAmountsIn" })?;

    let tokens: Vec<Token> = tokens_arr
        .iter()
        .map(|v| {
            let addr = v.as_str().and_then(|s| s.parse().ok()).unwrap_or(Address::ZERO);
            token_metadata(addr, ctx.chain_id)
        })
        .collect();
    let amounts: Vec<AmountSpec> = max_amounts
        .iter()
        .map(|v| amount_max(v.as_str().unwrap_or("0").to_string()))
        .collect();

    Ok(LiquidityFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v3".into()],
        tokens,
        amounts,
        position_token_id: Some(format!("pool-{pool_addr:#x}")),
        fee_tier: None,
        tick_lower: None,
        tick_upper: None,
        collect_max: None,
        recipients: recipients_from(None, ctx.actor),
        deadlines: DeadlineFields {
            deadline: None,
            deadline_horizon_seconds: None,
        },
    })
}

pub fn build_balancer_v3_remove_liquidity_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LiquidityFields, SemiAdapterError> {
    let params = args.get("params").unwrap_or(args);
    let pool_addr = parse_addr_field(params.get("pool"), "params.pool")?;
    let tokens_arr = params
        .get("tokensOut")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "params.tokensOut" })?;
    let min_amounts = params
        .get("minAmountsOut")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "params.minAmountsOut" })?;

    let tokens: Vec<Token> = tokens_arr
        .iter()
        .map(|v| {
            let addr = v.as_str().and_then(|s| s.parse().ok()).unwrap_or(Address::ZERO);
            token_metadata(addr, ctx.chain_id)
        })
        .collect();
    let amounts: Vec<AmountSpec> = min_amounts
        .iter()
        .map(|v| amount_min(v.as_str().unwrap_or("0").to_string()))
        .collect();

    Ok(LiquidityFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v3".into()],
        tokens,
        amounts,
        position_token_id: Some(format!("pool-{pool_addr:#x}")),
        fee_tier: None,
        tick_lower: None,
        tick_upper: None,
        collect_max: None,
        recipients: recipients_from(None, ctx.actor),
        deadlines: DeadlineFields {
            deadline: None,
            deadline_horizon_seconds: None,
        },
    })
}

// ===========================================================================
// V3 BatchRouter — 멀티홉 ⭐
// ===========================================================================

/// V3 BatchRouter `swapExactIn(SwapPathExactAmountIn[] paths, ...)` 또는 `swapExactOut(...)`.
///
/// `SwapPathExactAmountIn = { tokenIn, steps: SwapPathStep[], exactAmountIn, minAmountOut }`.
/// `SwapPathStep = { pool, tokenOut, isBuffer }`.
///
/// 매핑 규칙:
/// - `paths.length == 1` + `steps.length == 1` → `SwapRoute::SingleHop`
/// - `paths.length == 1` + `steps.length > 1`  → `SwapRoute::MultiHop` (선형)
/// - `paths.length > 1`                         → `SwapRoute::Split` (path들을 branches로 flatten)
pub fn build_balancer_v3_batch_router_swap_fields(
    args: &Value,
    ctx: &BuildContext,
    mode: SwapMode,
) -> Result<SwapFields, SemiAdapterError> {
    let paths = args
        .get("paths")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "paths" })?;
    let deadline = as_u64(args, "deadline").ok();

    if paths.is_empty() {
        return Err(SemiAdapterError::AbiDecode {
            reason: "paths empty".into(),
        });
    }

    // 각 path → (token_in, steps, token_out 마지막)
    let mut all_input_tokens: Vec<Token> = Vec::new();
    let mut all_output_tokens: Vec<Token> = Vec::new();
    let mut all_hops_per_path: Vec<Vec<HopRef>> = Vec::with_capacity(paths.len());
    let mut total_amount_given: u128 = 0;
    let mut total_amount_limit: u128 = 0;

    for (path_i, path) in paths.iter().enumerate() {
        let token_in_addr = parse_addr_field(path.get("tokenIn"), "paths.tokenIn")?;
        let token_in = token_metadata(token_in_addr, ctx.chain_id);

        let steps_arr = path
            .get("steps")
            .and_then(|v| v.as_array())
            .ok_or(SemiAdapterError::MissingArg { name: "paths.steps" })?;
        if steps_arr.is_empty() {
            continue;
        }

        let mut prev_token = token_in.clone();
        let mut path_hops: Vec<HopRef> = Vec::with_capacity(steps_arr.len());
        for (i, step) in steps_arr.iter().enumerate() {
            let pool_addr = parse_addr_field(step.get("pool"), "paths.steps.pool")?;
            let token_out_addr = parse_addr_field(step.get("tokenOut"), "paths.steps.tokenOut")?;
            let token_out = token_metadata(token_out_addr, ctx.chain_id);
            let is_buffer = step.get("isBuffer").and_then(|v| v.as_bool()).unwrap_or(false);

            path_hops.push(HopRef {
                id: format!("h#{path_i}.{i}"),
                protocol: if is_buffer { "balancer.v3.buffer".into() } else { "balancer.v3".into() },
                token_in: prev_token,
                token_out: token_out.clone(),
                pool: Some(format!("t#pool-{pool_addr:#x}")),
                fee_bps: None,
                confidence: Confidence::High,
            });
            prev_token = token_out;
        }

        all_input_tokens.push(token_in);
        all_output_tokens.push(prev_token);

        // amount 계산: ExactIn은 exactAmountIn, ExactOut는 exactAmountOut
        match mode {
            SwapMode::ExactIn => {
                let exact = path.get("exactAmountIn").and_then(|v| v.as_str()).unwrap_or("0");
                total_amount_given = total_amount_given.saturating_add(exact.parse().unwrap_or(0));
                let min_out = path.get("minAmountOut").and_then(|v| v.as_str()).unwrap_or("0");
                total_amount_limit = total_amount_limit.saturating_add(min_out.parse().unwrap_or(0));
            }
            _ => {
                let exact = path.get("exactAmountOut").and_then(|v| v.as_str()).unwrap_or("0");
                total_amount_given = total_amount_given.saturating_add(exact.parse().unwrap_or(0));
                let max_in = path.get("maxAmountIn").and_then(|v| v.as_str()).unwrap_or("0");
                total_amount_limit = total_amount_limit.saturating_add(max_in.parse().unwrap_or(0));
            }
        }

        all_hops_per_path.push(path_hops);
    }

    // route 결정
    let route = if all_hops_per_path.len() == 1 {
        let single_path = all_hops_per_path.into_iter().next().unwrap();
        if single_path.len() == 1 {
            SwapRoute::SingleHop {
                hop: single_path.into_iter().next().unwrap(),
            }
        } else {
            SwapRoute::MultiHop { hops: single_path }
        }
    } else {
        // multi-path → flatten 후 Split의 branches에 모두 평면화
        // (진정한 nested 표현은 v0.2)
        let flat: Vec<HopRef> = all_hops_per_path.into_iter().flatten().collect();
        SwapRoute::Split { branches: flat }
    };

    let (amount_in, amount_out) = match mode {
        SwapMode::ExactIn => (
            AmountSpec { raw: total_amount_given.to_string(), kind: AmountKind::Exact },
            AmountSpec { raw: total_amount_limit.to_string(), kind: AmountKind::Min },
        ),
        _ => (
            AmountSpec { raw: total_amount_limit.to_string(), kind: AmountKind::Max },
            AmountSpec { raw: total_amount_given.to_string(), kind: AmountKind::Exact },
        ),
    };

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v3".into()],
        input_tokens: all_input_tokens,
        output_tokens: all_output_tokens,
        mode,
        amount_in,
        amount_out: amount_out.clone(),
        route,
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: matches!(mode, SwapMode::ExactIn).then(|| amount_out.clone()),
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(None, ctx.actor),
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, ctx.block_timestamp)),
        },
        max_fee_bps: None,
        has_zero_min_output: matches!(mode, SwapMode::ExactIn) && amount_out.raw == "0",
    })
}

fn parse_addr_field(v: Option<&Value>, name: &'static str) -> Result<Address, SemiAdapterError> {
    let s = v
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name })?;
    s.parse().map_err(|_| SemiAdapterError::BadAddress { value: s.into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> BuildContext<'static> {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let target: Address = VAULT_MAINNET_HEX.parse().unwrap();
        BuildContext {
            chain_id: 1,
            actor,
            target,
            value_wei: "0".into(),
            block_timestamp: Some(1_762_499_000),
            targets: &[],
        }
    }

    #[test]
    fn v3_vault_swap_single() {
        let ctx = ctx();
        let args = serde_json::json!({
            "params": {
                "kind": 0,
                "pool": "0x1234567890123456789012345678901234567890",
                "tokenIn": "0x6B175474E89094C44Da98b954EedeAC495271d0F",
                "tokenOut": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "amountGivenRaw": "1000000000000000000",
                "limitRaw": "990000",
                "userData": "0x"
            }
        });
        let fields = build_balancer_v3_swap_fields(&args, &ctx).unwrap();
        assert_eq!(fields.mode, SwapMode::ExactIn);
        assert_eq!(fields.input_tokens[0].symbol, "DAI");
        assert_eq!(fields.output_tokens[0].symbol, "USDC");
        assert!(matches!(fields.route, SwapRoute::SingleHop { .. }));
    }

    #[test]
    fn v3_batch_router_multi_hop_single_path() {
        // USDC → WETH → WBTC (2 step, 1 path)
        let ctx = ctx();
        let args = serde_json::json!({
            "paths": [
                {
                    "tokenIn": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                    "steps": [
                        { "pool": "0x1111111111111111111111111111111111111111", "tokenOut": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "isBuffer": false },
                        { "pool": "0x2222222222222222222222222222222222222222", "tokenOut": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", "isBuffer": false }
                    ],
                    "exactAmountIn": "1000000000",
                    "minAmountOut": "3000000"
                }
            ],
            "deadline": "1762500000",
            "wethIsEth": false,
            "userData": "0x"
        });
        let fields = build_balancer_v3_batch_router_swap_fields(&args, &ctx, SwapMode::ExactIn).unwrap();
        assert_eq!(fields.mode, SwapMode::ExactIn);
        assert_eq!(fields.input_tokens[0].symbol, "USDC");
        assert_eq!(fields.output_tokens[0].symbol, "WBTC");
        if let SwapRoute::MultiHop { hops } = &fields.route {
            assert_eq!(hops.len(), 2);
            assert_eq!(hops[0].protocol, "balancer.v3");
            assert_eq!(hops[0].token_out.symbol, "WETH");
            assert_eq!(hops[1].token_out.symbol, "WBTC");
        } else {
            panic!("expected MultiHop");
        }
    }

    #[test]
    fn v3_batch_router_split_multi_path() {
        // 두 path가 같은 USDC → WETH로 가는 경우 → Split
        let ctx = ctx();
        let args = serde_json::json!({
            "paths": [
                {
                    "tokenIn": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                    "steps": [{ "pool": "0x1111111111111111111111111111111111111111", "tokenOut": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "isBuffer": false }],
                    "exactAmountIn": "600000000",
                    "minAmountOut": "200000000000000000"
                },
                {
                    "tokenIn": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                    "steps": [{ "pool": "0x3333333333333333333333333333333333333333", "tokenOut": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "isBuffer": false }],
                    "exactAmountIn": "400000000",
                    "minAmountOut": "130000000000000000"
                }
            ],
            "deadline": "1762500000"
        });
        let fields = build_balancer_v3_batch_router_swap_fields(&args, &ctx, SwapMode::ExactIn).unwrap();
        assert!(matches!(fields.route, SwapRoute::Split { .. }));
        assert_eq!(fields.amount_in.raw, "1000000000"); // 600 + 400
    }
}
