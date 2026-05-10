//! PancakeSwap decoder (V2 / V3 / SmartRouter / UR / Infinity).
//!
//! V2와 V3는 Uniswap fork이므로 `uniswap_v2`/`uniswap_v3`의 디코더 패턴을
//! 재사용하되 `protocol_ids`를 `["pancakeswap"]`으로 교체하고 Extension은
//! `pancakeswap` 통합 namespace + `data.component`로 식별.
//!
//! UR은 마스크가 0x3f (Uniswap UR과 다름).

use serde_json::Value;

use crate::action::fields::{AggregationFields, SwapFields};
use crate::semi_adapter::common::{as_u64, deadline_horizon};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::uniswap_ur::{parse_commands, UrOpcode};
use crate::semi_adapter::uniswap_v2::route_v2_swap;
use crate::semi_adapter::uniswap_v3::build_v3_swap_fields;
use crate::semi_adapter::BuildContext;
use crate::types::DeadlineFields;

/// Component 식별자 (Extension `data.component`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PancakeComponent {
    V2,
    V3,
    SmartRouter,
    UniversalRouter,
    Infinity,
}

impl PancakeComponent {
    pub fn as_str(self) -> &'static str {
        match self {
            PancakeComponent::V2 => "v2",
            PancakeComponent::V3 => "v3",
            PancakeComponent::SmartRouter => "smartRouter",
            PancakeComponent::UniversalRouter => "universalRouter",
            PancakeComponent::Infinity => "infinity",
        }
    }
}

/// V2 swap (Uniswap V2 fork — selector 동일).
pub fn build_pancake_v2_swap_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let mut fields = route_v2_swap(selector, args, ctx)?;
    fields.protocol_ids = vec!["pancakeswap".into()];
    if let crate::action::fields::SwapRoute::SingleHop { hop } = &mut fields.route {
        hop.protocol = "pancakeswap.v2".into();
    } else if let crate::action::fields::SwapRoute::MultiHop { hops } = &mut fields.route {
        for h in hops {
            h.protocol = "pancakeswap.v2".into();
        }
    }
    Ok(fields)
}

/// V3 swap (Uniswap V3 fork — `pancakeV3SwapCallback`만 차이).
pub fn build_pancake_v3_swap_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let mut fields = build_v3_swap_fields(selector, args, ctx)?;
    fields.protocol_ids = vec!["pancakeswap".into()];
    if let crate::action::fields::SwapRoute::SingleHop { hop } = &mut fields.route {
        hop.protocol = "pancakeswap.v3".into();
    } else if let crate::action::fields::SwapRoute::MultiHop { hops } = &mut fields.route {
        for h in hops {
            h.protocol = "pancakeswap.v3".into();
        }
    }
    Ok(fields)
}

/// PancakeSwap UR `execute` — 마스크 0x3f.
pub fn build_pancake_ur_aggregation_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<AggregationFields, SemiAdapterError> {
    let commands_hex = args
        .get("commands")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "commands" })?
        .to_string();
    let inputs_len = args
        .get("inputs")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let deadline = as_u64(args, "deadline").ok();

    // 마스크는 0x3f, opcode 공간이 Uniswap UR과 다르지만 동일 enum 재사용 (UNKNOWN으로 fallback)
    let commands = parse_commands(&commands_hex, 0x3f)?;
    let promoted = commands
        .iter()
        .filter(|(op, _)| {
            // PancakeSwap UR 한정 promote 규칙 — V2/V3/INFI swap + STABLE_SWAP
            matches!(
                op,
                UrOpcode::V3SwapExactIn | UrOpcode::V3SwapExactOut | UrOpcode::V2SwapExactIn
                | UrOpcode::V2SwapExactOut | UrOpcode::V4Swap // 0x10 = INFI_SWAP
            ) || matches!(op, UrOpcode::Unknown(0x22) | UrOpcode::Unknown(0x23)) // STABLE_SWAP_*
        })
        .count();

    Ok(AggregationFields {
        actor: ctx.actor,
        protocol_ids: vec!["pancakeswap".into()],
        family: "pancakeswap".into(),
        commands_hex,
        mask: 0x3f,
        child_count: promoted.max(inputs_len),
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, ctx.block_timestamp)),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::Confidence;
    use crate::semi_adapter::uniswap_v2::SEL_SWAP_EXACT_TOKENS_FOR_TOKENS;
    use crate::target::{ContractTarget, DiscoveredBy, TargetRole, Verification};
    use crate::types::Address;
    use serde_json::json;

    fn ctx_bsc() -> (Vec<ContractTarget>, BuildContext<'static>) {
        let target: Address = "0x10ED43C718714eb63d5aA57B78B54704E256024E"
            .parse()
            .unwrap();
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let targets: Vec<ContractTarget> = vec![ContractTarget {
            id: "t#router".into(),
            address: target,
            chain_id: 56,
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
                chain_id: 56,
                actor,
                target,
                value_wei: "0".into(),
                block_timestamp: Some(1_762_499_000),
                targets: leaked,
            },
        )
    }

    #[test]
    fn pancake_v2_protocol_id() {
        let (_t, ctx) = ctx_bsc();
        let args = json!({
            "amountIn": "100000000000000000000",
            "amountOutMin": "30000000000000000000",
            "path": [
                "0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56",
                "0x0E09FaBB73Bd3Ade0a17ECC321fD13a19e81cE82"
            ],
            "to": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000"
        });
        let fields =
            build_pancake_v2_swap_fields(&SEL_SWAP_EXACT_TOKENS_FOR_TOKENS, &args, &ctx).unwrap();
        assert_eq!(fields.protocol_ids, vec!["pancakeswap"]);
    }

    #[test]
    fn pancake_ur_mask_0x3f() {
        let (_t, ctx) = ctx_bsc();
        let args = json!({
            "commands": "0x10",
            "inputs": ["0x"],
            "deadline": "1762500000"
        });
        let fields = build_pancake_ur_aggregation_fields(&args, &ctx).unwrap();
        assert_eq!(fields.mask, 0x3f);
        assert_eq!(fields.family, "pancakeswap");
    }
}
