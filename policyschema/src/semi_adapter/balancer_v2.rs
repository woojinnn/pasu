//! Balancer V2 Vault — singleton 모델, `bytes32 poolId` 기반.
//!
//! 5 핵심 함수:
//! - `swap(SingleSwap, FundManagement, uint256 limit, uint256 deadline)` — single hop
//! - `batchSwap(SwapKind, BatchSwapStep[], IAsset[], FundManagement, int256[] limits, uint256 deadline)` — **멀티홉 그래프** ⭐
//! - `joinPool(bytes32 poolId, address sender, address recipient, JoinPoolRequest)` — LP 추가
//! - `exitPool(bytes32 poolId, address sender, address recipient, ExitPoolRequest)` — LP 회수
//! - `flashLoan(IFlashLoanRecipient, IERC20[], uint256[], bytes)`
//!
//! `poolId`: 앞 20B = pool address, 다음 2B = specialization, 뒤 10B = nonce.

use serde_json::Value;

use crate::action::fields::{
    HopRef, LendingFields, LiquidityFields, SettlementKind, SlippageInfo, SlippageSource,
    SwapFields, SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_max, amount_min, as_address, as_u64, as_uint_string, deadline_horizon,
    recipients_from,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{Address, AmountSpec, DeadlineFields, RecipientFields, Token};

pub const VAULT_MAINNET_HEX: &str = "0xBA12222222228d8Ba445958a75a0704d566BF2C8";

// 셀렉터 (참조: Balancer V2 IVault ABI)
pub const SEL_SWAP: [u8; 4] = [0x52, 0xbb, 0xbe, 0x29];
pub const SEL_BATCH_SWAP: [u8; 4] = [0x94, 0x5b, 0xce, 0xc9];
pub const SEL_JOIN_POOL: [u8; 4] = [0xb9, 0x5c, 0xac, 0x28];
pub const SEL_EXIT_POOL: [u8; 4] = [0x8b, 0xdb, 0x39, 0x13];
pub const SEL_FLASH_LOAN: [u8; 4] = [0x5c, 0x38, 0x44, 0x9e];

/// `bytes32 poolId`에서 앞 20B를 풀 주소로 추출.
pub fn pool_id_to_address(pool_id_hex: &str) -> Result<Address, SemiAdapterError> {
    let trimmed = pool_id_hex.trim_start_matches("0x");
    if trimmed.len() != 64 {
        return Err(SemiAdapterError::AbiDecode {
            reason: format!("poolId must be 32 bytes hex, got {} chars", trimmed.len()),
        });
    }
    let addr_hex = &trimmed[0..40];
    let bytes = hex::decode(addr_hex).map_err(|e| SemiAdapterError::BadHex(e.to_string()))?;
    Ok(Address::from_slice(&bytes))
}

// ===========================================================================
// swap (single hop)
// ===========================================================================

/// `swap(SingleSwap, FundManagement, uint256 limit, uint256 deadline)`.
///
/// `SingleSwap = { poolId: bytes32, kind: SwapKind, assetIn: IAsset, assetOut: IAsset, amount: uint256, userData: bytes }`.
pub fn build_balancer_v2_swap_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let single = args
        .get("singleSwap")
        .or_else(|| args.get("request"))
        .ok_or(SemiAdapterError::MissingArg { name: "singleSwap" })?;

    let pool_id = single
        .get("poolId")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "singleSwap.poolId" })?;
    let kind_n = single.get("kind").and_then(|v| v.as_u64()).unwrap_or(0);
    let mode = if kind_n == 0 { SwapMode::ExactIn } else { SwapMode::ExactOut };

    let asset_in_addr = parse_iasset(single.get("assetIn"))?;
    let asset_out_addr = parse_iasset(single.get("assetOut"))?;
    let amount_raw = single
        .get("amount")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or(SemiAdapterError::MissingArg { name: "singleSwap.amount" })?;

    let limit_raw = as_uint_string(args, "limit")?;
    let deadline = as_u64(args, "deadline")?;
    let funds = args
        .get("funds")
        .ok_or(SemiAdapterError::MissingArg { name: "funds" })?;
    let recipient = parse_address(funds.get("recipient"), "funds.recipient")?;

    let token_in = token_metadata(asset_in_addr, ctx.chain_id);
    let token_out = token_metadata(asset_out_addr, ctx.chain_id);
    let pool_addr = pool_id_to_address(pool_id)?;

    let (amount_in, amount_out) = match mode {
        SwapMode::ExactIn => (amount_exact(amount_raw), amount_min(limit_raw)),
        _ => (amount_max(limit_raw), amount_exact(amount_raw)),
    };

    let hop = HopRef {
        id: "h#0".into(),
        protocol: "balancer.v2".into(),
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        pool: Some(format!("t#pool-{pool_addr:#x}")),
        fee_bps: None, // V2는 풀별 fee — userData/registry 의존
        confidence: Confidence::High,
    };

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v2".into()],
        input_tokens: vec![token_in],
        output_tokens: vec![token_out],
        mode,
        amount_in: amount_in.clone(),
        amount_out: amount_out.clone(),
        route: SwapRoute::SingleHop { hop },
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: matches!(mode, SwapMode::ExactIn).then(|| amount_out.clone()),
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline: Some(deadline),
            deadline_horizon_seconds: deadline_horizon(deadline, ctx.block_timestamp),
        },
        max_fee_bps: None,
        has_zero_min_output: matches!(mode, SwapMode::ExactIn) && amount_out.raw == "0",
    })
}

// ===========================================================================
// batchSwap (멀티홉 그래프) ⭐
// ===========================================================================

/// `batchSwap(SwapKind kind, BatchSwapStep[] steps, IAsset[] assets, FundManagement funds, int256[] limits, uint256 deadline)`.
///
/// 각 step은 `(poolId, assetInIndex, assetOutIndex, amount, userData)`. N step → N hop의 *그래프*.
/// Uniswap의 1차원 path와 달리 임의 그래프 traverse 가능.
pub fn build_balancer_v2_batch_swap_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let kind_n = args.get("kind").and_then(|v| v.as_u64()).unwrap_or(0);
    let mode = if kind_n == 0 { SwapMode::ExactIn } else { SwapMode::ExactOut };

    let steps = args
        .get("swaps")
        .or_else(|| args.get("steps"))
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "swaps" })?;
    let assets = args
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "assets" })?;
    let limits = args
        .get("limits")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "limits" })?;
    let funds = args
        .get("funds")
        .ok_or(SemiAdapterError::MissingArg { name: "funds" })?;
    let recipient = parse_address(funds.get("recipient"), "funds.recipient")?;
    let deadline = as_u64(args, "deadline")?;

    // assets[i] → Token 매핑
    let asset_tokens: Vec<Token> = assets
        .iter()
        .map(|v| {
            let addr = v
                .as_str()
                .and_then(|s| s.parse::<Address>().ok())
                .unwrap_or(Address::ZERO);
            token_metadata(addr, ctx.chain_id)
        })
        .collect();

    // step → HopRef
    let mut hops: Vec<HopRef> = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let pool_id = step
            .get("poolId")
            .and_then(|v| v.as_str())
            .ok_or(SemiAdapterError::MissingArg { name: "swaps.poolId" })?;
        let in_idx = step
            .get("assetInIndex")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let out_idx = step
            .get("assetOutIndex")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let pool_addr = pool_id_to_address(pool_id)?;
        let token_in = asset_tokens
            .get(in_idx)
            .cloned()
            .unwrap_or_else(|| token_metadata(Address::ZERO, ctx.chain_id));
        let token_out = asset_tokens
            .get(out_idx)
            .cloned()
            .unwrap_or_else(|| token_metadata(Address::ZERO, ctx.chain_id));
        hops.push(HopRef {
            id: format!("h#{i}"),
            protocol: "balancer.v2".into(),
            token_in,
            token_out,
            pool: Some(format!("t#pool-{pool_addr:#x}")),
            fee_bps: None,
            confidence: Confidence::High,
        });
    }

    // net delta: limits 양수 = 사용자 지출, 음수 = 사용자 수령
    let mut input_tokens: Vec<Token> = Vec::new();
    let mut output_tokens: Vec<Token> = Vec::new();
    let mut total_in: i128 = 0;
    let mut total_out: i128 = 0;
    for (i, lim) in limits.iter().enumerate() {
        let v_str = lim.as_str().unwrap_or("0");
        let v_i128 = v_str.parse::<i128>().unwrap_or(0);
        if let Some(tok) = asset_tokens.get(i) {
            if v_i128 > 0 {
                input_tokens.push(tok.clone());
                total_in = total_in.saturating_add(v_i128);
            } else if v_i128 < 0 {
                output_tokens.push(tok.clone());
                total_out = total_out.saturating_sub(v_i128); // 음수의 절대값
            }
        }
    }

    // 최소 1개씩 보장 (limits가 모두 0인 케이스)
    if input_tokens.is_empty() && !hops.is_empty() {
        input_tokens.push(hops[0].token_in.clone());
    }
    if output_tokens.is_empty() && !hops.is_empty() {
        output_tokens.push(hops[hops.len() - 1].token_out.clone());
    }

    let amount_in = AmountSpec {
        raw: total_in.max(0).to_string(),
        kind: if matches!(mode, SwapMode::ExactIn) {
            crate::types::AmountKind::Exact
        } else {
            crate::types::AmountKind::Max
        },
    };
    let amount_out = AmountSpec {
        raw: total_out.max(0).to_string(),
        kind: if matches!(mode, SwapMode::ExactIn) {
            crate::types::AmountKind::Min
        } else {
            crate::types::AmountKind::Exact
        },
    };

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v2".into()],
        input_tokens,
        output_tokens,
        mode,
        amount_in,
        amount_out: amount_out.clone(),
        route: SwapRoute::Batch { steps: hops },
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: matches!(mode, SwapMode::ExactIn).then(|| amount_out.clone()),
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline: Some(deadline),
            deadline_horizon_seconds: deadline_horizon(deadline, ctx.block_timestamp),
        },
        max_fee_bps: None,
        has_zero_min_output: matches!(mode, SwapMode::ExactIn) && amount_out.raw == "0",
    })
}

// ===========================================================================
// joinPool / exitPool
// ===========================================================================

pub fn build_balancer_v2_join_pool_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LiquidityFields, SemiAdapterError> {
    let pool_id = args
        .get("poolId")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "poolId" })?;
    let request = args
        .get("request")
        .ok_or(SemiAdapterError::MissingArg { name: "request" })?;
    let assets = request
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "request.assets" })?;
    let max_amounts = request
        .get("maxAmountsIn")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "request.maxAmountsIn" })?;
    let recipient = as_address(args, "recipient")?;
    let _pool_addr = pool_id_to_address(pool_id)?;

    let tokens: Vec<Token> = assets
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
        protocol_ids: vec!["balancer.v2".into()],
        tokens,
        amounts,
        position_token_id: None,
        fee_tier: None,
        tick_lower: None,
        tick_upper: None,
        collect_max: None,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline: None,
            deadline_horizon_seconds: None,
        },
    })
}

pub fn build_balancer_v2_exit_pool_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LiquidityFields, SemiAdapterError> {
    let pool_id = args
        .get("poolId")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "poolId" })?;
    let request = args
        .get("request")
        .ok_or(SemiAdapterError::MissingArg { name: "request" })?;
    let assets = request
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "request.assets" })?;
    let min_amounts = request
        .get("minAmountsOut")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "request.minAmountsOut" })?;
    let recipient = as_address(args, "recipient")?;
    let _pool_addr = pool_id_to_address(pool_id)?;

    let tokens: Vec<Token> = assets
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
        protocol_ids: vec!["balancer.v2".into()],
        tokens,
        amounts,
        position_token_id: None,
        fee_tier: None,
        tick_lower: None,
        tick_upper: None,
        collect_max: None,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline: None,
            deadline_horizon_seconds: None,
        },
    })
}

// ===========================================================================
// flashLoan
// ===========================================================================

pub fn build_balancer_v2_flash_loan_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LendingFields, SemiAdapterError> {
    let recipient = as_address(args, "recipient")?;
    let tokens_arr = args
        .get("tokens")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "tokens" })?;
    let amounts_arr = args
        .get("amounts")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "amounts" })?;

    let flash_assets: Vec<Token> = tokens_arr
        .iter()
        .map(|v| {
            let addr = v.as_str().and_then(|s| s.parse().ok()).unwrap_or(Address::ZERO);
            token_metadata(addr, ctx.chain_id)
        })
        .collect();
    let flash_amounts: Vec<AmountSpec> = amounts_arr
        .iter()
        .map(|v| amount_exact(v.as_str().unwrap_or("0").to_string()))
        .collect();

    let first_asset = flash_assets.first().cloned().unwrap_or_else(|| {
        token_metadata(Address::ZERO, ctx.chain_id)
    });
    let first_amount = flash_amounts.first().cloned().unwrap_or_else(|| amount_exact("0".to_string()));

    Ok(LendingFields {
        actor: ctx.actor,
        protocol_ids: vec!["balancer.v2".into()],
        asset: first_asset,
        amount: first_amount,
        on_behalf_of: ctx.actor,
        interest_rate_mode: None,
        use_as_collateral: None,
        e_mode_category_id: None,
        liquidation_target: None,
        collateral_asset: None,
        flash_assets: Some(flash_assets),
        flash_amounts: Some(flash_amounts),
        flash_modes: None,
        recipients: RecipientFields {
            recipient: Some(crate::types::RecipientRef::Address { address: recipient }),
            recipient_equals_actor: recipient == ctx.actor,
            has_external_recipient: recipient != ctx.actor,
        },
    })
}

// ===========================================================================
// helper
// ===========================================================================

fn parse_iasset(v: Option<&Value>) -> Result<Address, SemiAdapterError> {
    let s = v
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "IAsset" })?;
    s.parse().map_err(|_| SemiAdapterError::BadAddress { value: s.into() })
}

fn parse_address(v: Option<&Value>, name: &'static str) -> Result<Address, SemiAdapterError> {
    let s = v
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name })?;
    s.parse().map_err(|_| SemiAdapterError::BadAddress { value: s.into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semi_adapter::registry::token_metadata;

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
    fn pool_id_address_extract() {
        let pool_id = "0xa6f548df93de924d73be7d25228870f11c1b1ed7000000000000000000000044";
        let addr = pool_id_to_address(pool_id).unwrap();
        assert_eq!(format!("{addr:#x}").to_lowercase(), "0xa6f548df93de924d73be7d25228870f11c1b1ed7");
    }

    #[test]
    fn batch_swap_3_step() {
        let ctx = ctx();
        let args = serde_json::json!({
            "kind": 0,
            "swaps": [
                { "poolId": "0xa6f548df93de924d73be7d25228870f11c1b1ed7000000000000000000000044", "assetInIndex": 0, "assetOutIndex": 1, "amount": "1000000000", "userData": "0x" },
                { "poolId": "0xbf96189eee9357a95c7719f4f5047f76bde804e5000000000000000000000087", "assetInIndex": 1, "assetOutIndex": 2, "amount": "0", "userData": "0x" }
            ],
            "assets": [
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
                "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"
            ],
            "funds": {
                "sender": "0x1111111111111111111111111111111111111111",
                "fromInternalBalance": false,
                "recipient": "0x1111111111111111111111111111111111111111",
                "toInternalBalance": false
            },
            "limits": ["1000000000", "0", "-3000000"],
            "deadline": "1762500000"
        });
        let fields = build_balancer_v2_batch_swap_fields(&args, &ctx).unwrap();
        assert_eq!(fields.mode, SwapMode::ExactIn);
        assert_eq!(fields.protocol_ids, vec!["balancer.v2"]);
        // Batch route with 2 hops
        if let SwapRoute::Batch { steps } = &fields.route {
            assert_eq!(steps.len(), 2);
            assert_eq!(steps[0].protocol, "balancer.v2");
            assert_eq!(steps[0].token_in.symbol, "USDC");
            assert_eq!(steps[1].token_out.symbol, "WBTC");
        } else {
            panic!("expected Batch route");
        }
        // net delta: USDC in, WBTC out
        assert_eq!(fields.input_tokens.len(), 1);
        assert_eq!(fields.input_tokens[0].symbol, "USDC");
        assert_eq!(fields.output_tokens.len(), 1);
        assert_eq!(fields.output_tokens[0].symbol, "WBTC");
    }

    #[test]
    fn single_swap() {
        let _ = token_metadata; // suppress
        let ctx = ctx();
        let args = serde_json::json!({
            "singleSwap": {
                "poolId": "0x0b09dea16768f0799065c475be02919503cb2a3500020000000000000000001a",
                "kind": 0,
                "assetIn": "0x6B175474E89094C44Da98b954EedeAC495271d0F",
                "assetOut": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "amount": "1000000000000000000000",
                "userData": "0x"
            },
            "funds": {
                "sender": "0x1111111111111111111111111111111111111111",
                "fromInternalBalance": false,
                "recipient": "0x1111111111111111111111111111111111111111",
                "toInternalBalance": false
            },
            "limit": "990000000",
            "deadline": "1762500000"
        });
        let fields = build_balancer_v2_swap_fields(&args, &ctx).unwrap();
        assert_eq!(fields.input_tokens[0].symbol, "DAI");
        assert_eq!(fields.output_tokens[0].symbol, "USDC");
        assert!(matches!(fields.route, SwapRoute::SingleHop { .. }));
    }
}
