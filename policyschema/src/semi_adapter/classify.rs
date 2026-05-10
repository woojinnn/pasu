//! 세미-어댑터 측 분류 진입점.
//!
//! `(target_address, selector, args, ctx)` 튜플 → `ClassifyOutcome` 매핑.
//! 이 모듈은 *실행* 영역 — `src/dispatch.rs`의 *table 정의*(스키마 영역)와는
//! 분리됨. table 정의는 메타데이터, 실행은 여기서.

use serde_json::Value;

use crate::action::{ActionCategory, ActionFields, ActionType};
use crate::confidence::Confidence;
use crate::semi_adapter::{
    aave_v3, aerodrome_slipstream, aerodrome_v1, error::SemiAdapterError, lido, morpho_blue,
    pancakeswap, registry, uniswap_ur, uniswap_v2, uniswap_v3, uniswap_v4, BuildContext,
};
use crate::types::Address;

/// 분류 결과.
#[derive(Debug, Clone)]
pub struct ClassifyOutcome {
    pub category: ActionCategory,
    pub action_type: ActionType,
    pub fields: ActionFields,
    pub confidence: Confidence,
    pub promote: bool,
}

/// `(target_address, selector, chain_id, args)` → `ClassifyOutcome`.
///
/// Universal Router의 자식 자체는 `dispatch_ur_child`에서 처리.
pub fn classify_call(
    target_address: Address,
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<ClassifyOutcome, SemiAdapterError> {
    // 1. UR family인지 먼저 확인 (mask 다름)
    if let Some(family) = registry::ur_family_for(target_address, ctx.chain_id) {
        return classify_ur_execute(family, selector, args, ctx);
    }

    // 2. selector 매칭으로 프로토콜 식별
    // (실제 프로덕션은 큐레이트 registry로 target → 프로토콜 식별 — 여기선 selector로 fallback)
    classify_by_selector(selector, args, ctx)
}

fn classify_ur_execute(
    family: registry::UrFamily,
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<ClassifyOutcome, SemiAdapterError> {
    let agg = match family {
        registry::UrFamily::Uniswap => uniswap_ur::build_ur_aggregation_fields(selector, args, ctx)?,
        registry::UrFamily::Pancakeswap => pancakeswap::build_pancake_ur_aggregation_fields(args, ctx)?,
    };
    Ok(ClassifyOutcome {
        category: ActionCategory::Aggregation,
        action_type: ActionType::RouterPlan,
        fields: ActionFields::Aggregation(agg),
        confidence: Confidence::High,
        promote: false,
    })
}

fn classify_by_selector(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<ClassifyOutcome, SemiAdapterError> {
    use uniswap_v2::*;
    use uniswap_v3::*;

    let v2_sels = [
        SEL_SWAP_EXACT_TOKENS_FOR_TOKENS,
        SEL_SWAP_TOKENS_FOR_EXACT_TOKENS,
        SEL_SWAP_EXACT_ETH_FOR_TOKENS,
        SEL_SWAP_TOKENS_FOR_EXACT_ETH,
        SEL_SWAP_EXACT_TOKENS_FOR_ETH,
        SEL_SWAP_ETH_FOR_EXACT_TOKENS,
        SEL_SWAP_EXACT_TOKENS_FOR_TOKENS_FOT,
        SEL_SWAP_EXACT_ETH_FOR_TOKENS_FOT,
        SEL_SWAP_EXACT_TOKENS_FOR_ETH_FOT,
    ];
    if v2_sels.contains(selector) {
        let f = route_v2_swap(selector, args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            fields: ActionFields::Swap(f),
            confidence: Confidence::High,
            promote: true,
        });
    }

    if [
        SEL_EXACT_INPUT_SINGLE,
        SEL_EXACT_INPUT,
        SEL_EXACT_OUTPUT_SINGLE,
        SEL_EXACT_OUTPUT,
    ]
    .contains(selector)
    {
        let f = build_v3_swap_fields(selector, args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            fields: ActionFields::Swap(f),
            confidence: Confidence::High,
            promote: true,
        });
    }

    if *selector == aerodrome_v1::SEL_SWAP_EXACT_TOKENS_FOR_TOKENS {
        let f = aerodrome_v1::build_aerodrome_v1_swap_fields(selector, args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            fields: ActionFields::Swap(f),
            confidence: Confidence::High,
            promote: true,
        });
    }

    if [
        aave_v3::SEL_SUPPLY,
        aave_v3::SEL_WITHDRAW,
        aave_v3::SEL_BORROW,
        aave_v3::SEL_REPAY,
    ]
    .contains(selector)
    {
        let action_type = match *selector {
            aave_v3::SEL_SUPPLY => ActionType::Supply,
            aave_v3::SEL_WITHDRAW => ActionType::WithdrawCollateral,
            aave_v3::SEL_BORROW => ActionType::Borrow,
            aave_v3::SEL_REPAY => ActionType::Repay,
            _ => unreachable!(),
        };
        let f = aave_v3::build_aave_v3_lending_fields(selector, args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Lending,
            action_type,
            fields: ActionFields::Lending(f),
            confidence: Confidence::High,
            promote: true,
        });
    }

    if *selector == morpho_blue::SEL_SUPPLY {
        let f = morpho_blue::build_morpho_supply_fields(args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Lending,
            action_type: ActionType::Supply,
            fields: ActionFields::Lending(f),
            confidence: Confidence::High,
            promote: true,
        });
    }

    if *selector == lido::SEL_SUBMIT {
        let f = lido::build_lido_stake_fields(args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::LiquidStaking,
            action_type: ActionType::Stake,
            fields: ActionFields::LiquidStaking(f),
            confidence: Confidence::High,
            promote: true,
        });
    }
    if *selector == lido::SEL_REQUEST_WITHDRAWALS {
        let f = lido::build_lido_request_withdrawal_fields(args, ctx)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::LiquidStaking,
            action_type: ActionType::UnstakeRequest,
            fields: ActionFields::LiquidStaking(f),
            confidence: Confidence::High,
            promote: true,
        });
    }
    if *selector == lido::SEL_WRAP_WSTETH {
        let f = lido::build_wsteth_wrap_fields(selector, args, ctx, true)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Swap,
            action_type: ActionType::Wrap,
            fields: ActionFields::Swap(f),
            confidence: Confidence::High,
            promote: true,
        });
    }
    if *selector == lido::SEL_UNWRAP_WSTETH {
        let f = lido::build_wsteth_wrap_fields(selector, args, ctx, false)?;
        return Ok(ClassifyOutcome {
            category: ActionCategory::Swap,
            action_type: ActionType::Unwrap,
            fields: ActionFields::Swap(f),
            confidence: Confidence::High,
            promote: true,
        });
    }

    Err(SemiAdapterError::DispatchMiss {
        key: format!("0x{}", hex::encode(selector)),
    })
}

/// `aerodrome_slipstream`: V3와 selector 충돌이 있어 명시 진입점.
/// v0.2에서 target 주소 기반 사전 분기 추가 예정.
pub fn classify_slipstream(
    args: &Value,
    ctx: &BuildContext,
) -> Result<ClassifyOutcome, SemiAdapterError> {
    let f = aerodrome_slipstream::build_slipstream_swap_fields(args, ctx)?;
    Ok(ClassifyOutcome {
        category: ActionCategory::Swap,
        action_type: ActionType::Swap,
        fields: ActionFields::Swap(f),
        confidence: Confidence::High,
        promote: true,
    })
}

/// V4 swap 분류 (UR opcode 0x10 V4_SWAP을 통해 호출되는 경우).
pub fn classify_v4_swap(
    args: &Value,
    ctx: &BuildContext,
) -> Result<ClassifyOutcome, SemiAdapterError> {
    let f = uniswap_v4::build_v4_swap_fields(args, ctx)?;
    Ok(ClassifyOutcome {
        category: ActionCategory::Swap,
        action_type: ActionType::Swap,
        fields: ActionFields::Swap(f),
        confidence: Confidence::High,
        promote: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::Confidence;
    use crate::target::{ContractTarget, DiscoveredBy, TargetRole, Verification};
    use serde_json::json;

    fn ctx_for(actor: Address, target: Address, chain_id: u64) -> (Vec<ContractTarget>, BuildContext<'static>) {
        let targets: Vec<ContractTarget> = vec![ContractTarget {
            id: "t#router".into(),
            address: target,
            chain_id,
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
                chain_id,
                actor,
                target,
                value_wei: "0".into(),
                block_timestamp: Some(1_762_499_000),
                targets: leaked,
            },
        )
    }

    #[test]
    fn classify_v2_swap() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let target: Address = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".parse().unwrap();
        let (_t, ctx) = ctx_for(actor, target, 1);
        let args = json!({
            "amountIn": "1000000000",
            "amountOutMin": "300000000000000000",
            "path": [
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            ],
            "to": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000"
        });
        let outcome = classify_call(target, &uniswap_v2::SEL_SWAP_EXACT_TOKENS_FOR_TOKENS, &args, &ctx).unwrap();
        assert_eq!(outcome.category, ActionCategory::Swap);
        assert_eq!(outcome.action_type, ActionType::Swap);
        assert!(outcome.promote);
    }

    #[test]
    fn classify_ur_execute_test() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let ur: Address = "0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af".parse().unwrap();
        let (_t, ctx) = ctx_for(actor, ur, 1);
        let args = json!({
            "commands": "0x00",
            "inputs": ["0x"],
            "deadline": "1762500000"
        });
        let outcome = classify_call(ur, &uniswap_ur::SEL_EXECUTE_WITH_DEADLINE, &args, &ctx).unwrap();
        assert_eq!(outcome.category, ActionCategory::Aggregation);
        assert_eq!(outcome.action_type, ActionType::RouterPlan);
        assert!(!outcome.promote);
    }

    #[test]
    fn classify_aave_supply() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let pool: Address = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2".parse().unwrap();
        let (_t, ctx) = ctx_for(actor, pool, 1);
        let args = json!({
            "asset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "amount": "1000000000",
            "onBehalfOf": "0x1111111111111111111111111111111111111111",
            "referralCode": 0
        });
        let outcome = classify_call(pool, &aave_v3::SEL_SUPPLY, &args, &ctx).unwrap();
        assert_eq!(outcome.category, ActionCategory::Lending);
        assert_eq!(outcome.action_type, ActionType::Supply);
    }
}
