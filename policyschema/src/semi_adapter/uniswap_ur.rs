//! Uniswap Universal Router decoder.
//!
//! `execute(bytes commands, bytes[] inputs[, uint256 deadline])` 진입점.
//! 각 opcode는 자식 Action으로 분해되지만, 이 모듈은 **부모 AggregationFields**만
//! 디코드. 자식 opcode 디코딩은 v0.2 (S4 단계는 골격만).
//!
//! # opcode 마스킹
//!
//! - `command_type = byte & 0x7f`
//! - `byte & 0x80 != 0` → `FLAG_ALLOW_REVERT` (실패 허용)

use serde_json::Value;

use crate::action::fields::AggregationFields;
use crate::semi_adapter::common::{as_u64, deadline_horizon};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::BuildContext;
use crate::types::DeadlineFields;

pub const SEL_EXECUTE_WITH_DEADLINE: [u8; 4] = [0x35, 0x93, 0x56, 0x4c];
pub const SEL_EXECUTE: [u8; 4] = [0x24, 0x85, 0x6b, 0xc3];

/// 알려진 Uniswap UR opcode (mask 후).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrOpcode {
    V3SwapExactIn,        // 0x00
    V3SwapExactOut,       // 0x01
    Permit2TransferFrom,  // 0x02
    Permit2PermitBatch,   // 0x03
    Sweep,                // 0x04
    Transfer,             // 0x05
    PayPortion,           // 0x06
    V2SwapExactIn,        // 0x08
    V2SwapExactOut,       // 0x09
    Permit2Permit,        // 0x0a
    WrapEth,              // 0x0b
    UnwrapWeth,           // 0x0c
    BalanceCheckErc20,    // 0x0e
    V4Swap,               // 0x10
    V3PositionManagerCall, // 0x12
    ExecuteSubPlan,       // 0x21
    Unknown(u8),
}

impl UrOpcode {
    pub fn from_byte(masked: u8) -> Self {
        match masked {
            0x00 => UrOpcode::V3SwapExactIn,
            0x01 => UrOpcode::V3SwapExactOut,
            0x02 => UrOpcode::Permit2TransferFrom,
            0x03 => UrOpcode::Permit2PermitBatch,
            0x04 => UrOpcode::Sweep,
            0x05 => UrOpcode::Transfer,
            0x06 => UrOpcode::PayPortion,
            0x08 => UrOpcode::V2SwapExactIn,
            0x09 => UrOpcode::V2SwapExactOut,
            0x0a => UrOpcode::Permit2Permit,
            0x0b => UrOpcode::WrapEth,
            0x0c => UrOpcode::UnwrapWeth,
            0x0e => UrOpcode::BalanceCheckErc20,
            0x10 => UrOpcode::V4Swap,
            0x12 => UrOpcode::V3PositionManagerCall,
            0x21 => UrOpcode::ExecuteSubPlan,
            other => UrOpcode::Unknown(other),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            UrOpcode::V3SwapExactIn => "V3_SWAP_EXACT_IN",
            UrOpcode::V3SwapExactOut => "V3_SWAP_EXACT_OUT",
            UrOpcode::Permit2TransferFrom => "PERMIT2_TRANSFER_FROM",
            UrOpcode::Permit2PermitBatch => "PERMIT2_PERMIT_BATCH",
            UrOpcode::Sweep => "SWEEP",
            UrOpcode::Transfer => "TRANSFER",
            UrOpcode::PayPortion => "PAY_PORTION",
            UrOpcode::V2SwapExactIn => "V2_SWAP_EXACT_IN",
            UrOpcode::V2SwapExactOut => "V2_SWAP_EXACT_OUT",
            UrOpcode::Permit2Permit => "PERMIT2_PERMIT",
            UrOpcode::WrapEth => "WRAP_ETH",
            UrOpcode::UnwrapWeth => "UNWRAP_WETH",
            UrOpcode::BalanceCheckErc20 => "BALANCE_CHECK_ERC20",
            UrOpcode::V4Swap => "V4_SWAP",
            UrOpcode::V3PositionManagerCall => "V3_POSITION_MANAGER_CALL",
            UrOpcode::ExecuteSubPlan => "EXECUTE_SUB_PLAN",
            UrOpcode::Unknown(_) => "UNKNOWN",
        }
    }

    /// 자식 Action으로 promote 할지. swap·sign은 promote, sweep·wrap·transfer 등은 false.
    pub fn promotes_to_action(self) -> bool {
        matches!(
            self,
            UrOpcode::V3SwapExactIn
                | UrOpcode::V3SwapExactOut
                | UrOpcode::V2SwapExactIn
                | UrOpcode::V2SwapExactOut
                | UrOpcode::V4Swap
                | UrOpcode::Permit2Permit
                | UrOpcode::Permit2PermitBatch
                | UrOpcode::ExecuteSubPlan
        )
    }
}

/// `commands` 바이트 → 마스킹된 opcode + allow_revert 플래그 시퀀스.
pub fn parse_commands(commands_hex: &str, mask: u8) -> Result<Vec<(UrOpcode, bool)>, SemiAdapterError> {
    let bytes = hex::decode(commands_hex.trim_start_matches("0x"))
        .map_err(|e| SemiAdapterError::BadHex(e.to_string()))?;
    let mut out = Vec::with_capacity(bytes.len());
    for b in bytes {
        let allow_revert = b & 0x80 != 0;
        let opcode = UrOpcode::from_byte(b & mask);
        out.push((opcode, allow_revert));
    }
    Ok(out)
}

/// `execute(...)` → 부모 RouterPlan AggregationFields. 자식 opcode 분해는 caller 책임.
pub fn build_ur_aggregation_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<AggregationFields, SemiAdapterError> {
    if *selector != SEL_EXECUTE_WITH_DEADLINE && *selector != SEL_EXECUTE {
        return Err(SemiAdapterError::BadSelector {
            expected: "0x3593564c or 0x24856bc3".into(),
            got: format!("0x{}", hex::encode(selector)),
        });
    }

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

    let deadline = if *selector == SEL_EXECUTE_WITH_DEADLINE {
        as_u64(args, "deadline").ok()
    } else {
        None
    };

    let commands = parse_commands(&commands_hex, 0x7f)?;
    let promoted = commands.iter().filter(|(op, _)| op.promotes_to_action()).count();

    Ok(AggregationFields {
        actor: ctx.actor,
        protocol_ids: vec!["uniswap.universalRouter".into()],
        family: "uniswap".into(),
        commands_hex,
        mask: 0x7f,
        child_count: promoted.max(inputs_len.min(promoted.max(1))),
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
    use crate::target::{ContractTarget, DiscoveredBy, TargetRole, Verification};
    use crate::types::Address;
    use serde_json::json;

    fn ctx() -> (Vec<ContractTarget>, BuildContext<'static>) {
        let target: Address = "0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af"
            .parse()
            .unwrap();
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let targets: Vec<ContractTarget> = vec![ContractTarget {
            id: "t#ur".into(),
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
    fn parse_commands_with_allow_revert() {
        // 0x80 (allow_revert) | 0x00 (V3_SWAP_EXACT_IN), 0x08 (V2_SWAP_EXACT_IN)
        let cmds = parse_commands("0x8008", 0x7f).unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].0, UrOpcode::V3SwapExactIn);
        assert!(cmds[0].1, "0x80 bit should set allow_revert");
        assert_eq!(cmds[1].0, UrOpcode::V2SwapExactIn);
        assert!(!cmds[1].1);
    }

    #[test]
    fn opcode_promotion() {
        assert!(UrOpcode::V3SwapExactIn.promotes_to_action());
        assert!(UrOpcode::Permit2Permit.promotes_to_action());
        assert!(!UrOpcode::WrapEth.promotes_to_action());
        assert!(!UrOpcode::Sweep.promotes_to_action());
    }

    #[test]
    fn execute_basic() {
        let (_t, ctx) = ctx();
        let args = json!({
            "commands": "0x0a00",
            "inputs": ["0x", "0x"],
            "deadline": "1762500000"
        });
        let fields = build_ur_aggregation_fields(&SEL_EXECUTE_WITH_DEADLINE, &args, &ctx).unwrap();
        assert_eq!(fields.family, "uniswap");
        assert_eq!(fields.commands_hex, "0x0a00");
        assert_eq!(fields.mask, 0x7f);
        // Permit2Permit + V3SwapExactIn 모두 promote
        assert_eq!(fields.child_count, 2);
    }
}
