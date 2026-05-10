//! Dispatch — 분류 룩업 *table* 정의 (스키마 영역).
//!
//! schema_v260508의 `dispatch.ts` 패턴 차용 — 이 파일은 *메타데이터*만 담는다.
//! `(selector, opcode, primaryType)` → `DispatchEntry` 매핑 정의.
//!
//! **실행 함수**(`classify_call` 등)는 [`crate::semi_adapter::classify`]에 있다 —
//! 스키마/세미-어댑터 경계 분리.

use crate::action::{ActionCategory, ActionType};
use crate::confidence::Confidence;
use crate::types::Address;

/// Dispatch 룩업 키 (3종).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DispatchKey {
    /// 트랜잭션 selector (4 byte).
    /// `protocol_scope`는 동일 selector가 여러 프로토콜에 매핑될 때
    /// 명시 (예: PancakeSwap V2가 Uniswap V2 selector 동일).
    Selector {
        selector: [u8; 4],
        protocol_scope: Option<&'static str>,
    },
    /// Universal Router 명령 (1 byte, masked).
    UrCommand {
        opcode: u8,
        family: UrFamily,
    },
    /// EIP-712 typed-data primary type.
    PrimaryType {
        verifying_contract: Address,
        primary_type: &'static str,
    },
}

/// Universal Router 패밀리 — opcode 마스킹 규칙이 다름.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UrFamily {
    /// Uniswap UR — `command & 0x7f`.
    Uniswap,
    /// PancakeSwap UR — `command & 0x3f`.
    Pancakeswap,
}

/// Dispatch 항목 — selector·opcode·primaryType이 어떤 ActionType으로 매핑되는지의 메타.
#[derive(Debug, Clone)]
pub struct DispatchEntry {
    pub category: ActionCategory,
    pub action_type: ActionType,
    /// 어떤 세미-어댑터를 호출할지의 식별자.
    pub semi_adapter: SemiAdapterId,
    /// 정적 분석에서 얻을 수 있는 신뢰도 상한.
    pub confidence: Confidence,
    /// 자식 Action으로 promote 할지 (UR opcode의 `WRAP_ETH`/`SWEEP` 등은 false).
    pub promote: bool,
    /// 사람용 메모 (출처·이슈·confidence ceiling 사유 등).
    pub notes: &'static str,
}

/// 세미-어댑터 식별자 — `DispatchEntry`가 어떤 빌더를 호출할지의 enum.
///
/// 각 variant는 `crate::semi_adapter::*` 모듈의 `build_*_fields` 함수와 1:1 대응.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemiAdapterId {
    UniswapV2Swap,
    UniswapV3Swap,
    UniswapV4Swap,
    UniswapUrExecute,
    PancakeswapV2Swap,
    PancakeswapV3Swap,
    PancakeswapUrExecute,
    PancakeswapInfinitySwap,
    AerodromeV1Swap,
    AerodromeSlipstreamSwap,
    AaveV3Lending,
    MorphoBlueLending,
    LidoStake,
    LidoUnstakeRequest,
    LidoClaimUnstake,
    LidoWstethWrap,
    LidoWstethUnwrap,
    SignTypedData,
}

// ===========================================================================
// Sub-table 정의 — 스키마 영역의 메타데이터
// ===========================================================================
//
// 각 sub-table은 한 프로토콜(또는 명명된 그룹)의 selector·opcode 매핑.
// `cargo build` 시점에 정적 검증되며, 세미-어댑터는 이 메타로 분류 결정.
//
// ※ 현재는 *공개 const 배열*. v0.2에서 keccak256 자동 검증을 도입할 때
//   `phf` 또는 `lazy_static` 기반 lookup으로 전환 가능.

/// Uniswap V2 Router02 (9 swap 변형).
pub const UNISWAP_V2_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0x38, 0xed, 0x17, 0x39],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapExactTokensForTokens",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x88, 0x03, 0xdb, 0xee],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapTokensForExactTokens",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x7f, 0xf3, 0x6a, 0xb5],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapExactETHForTokens (payable)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x4a, 0x25, 0xd9, 0x4a],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapTokensForExactETH",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x18, 0xcb, 0xaf, 0xe5],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapExactTokensForETH",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0xfb, 0x3b, 0xdb, 0x41],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapETHForExactTokens (payable)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x5c, 0x11, 0xd7, 0x95],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::Medium,
            promote: true,
            notes: "swapExactTokensForTokensSupportingFeeOnTransferTokens",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0xb6, 0xf9, 0xde, 0x95],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::Medium,
            promote: true,
            notes: "swapExactETHForTokensSupportingFeeOnTransferTokens",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x79, 0x1a, 0xc9, 0x47],
            protocol_scope: Some("uniswap.v2"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV2Swap,
            confidence: Confidence::Medium,
            promote: true,
            notes: "swapExactTokensForETHSupportingFeeOnTransferTokens",
        },
    ),
];

/// Uniswap V3 SwapRouter / SwapRouter02.
pub const UNISWAP_V3_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0x04, 0xe4, 0x5a, 0xaf],
            protocol_scope: Some("uniswap.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV3Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "exactInputSingle",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0xb8, 0x58, 0x18, 0x3f],
            protocol_scope: Some("uniswap.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV3Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "exactInput (encoded path)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x50, 0x23, 0xb4, 0xdf],
            protocol_scope: Some("uniswap.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV3Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "exactOutputSingle",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x09, 0xb8, 0x13, 0x46],
            protocol_scope: Some("uniswap.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::UniswapV3Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "exactOutput (encoded path, reverse)",
        },
    ),
];

/// Universal Router (Uniswap UR `execute` 진입점).
pub const UNISWAP_UR_SELECTOR_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0x35, 0x93, 0x56, 0x4c],
            protocol_scope: Some("uniswap.universalRouter"),
        },
        DispatchEntry {
            category: ActionCategory::Aggregation,
            action_type: ActionType::RouterPlan,
            semi_adapter: SemiAdapterId::UniswapUrExecute,
            confidence: Confidence::High,
            promote: false,
            notes: "execute(commands, inputs, deadline)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x24, 0x85, 0x6b, 0xc3],
            protocol_scope: Some("uniswap.universalRouter"),
        },
        DispatchEntry {
            category: ActionCategory::Aggregation,
            action_type: ActionType::RouterPlan,
            semi_adapter: SemiAdapterId::UniswapUrExecute,
            confidence: Confidence::High,
            promote: false,
            notes: "execute(commands, inputs)",
        },
    ),
];

/// Aerodrome V1 (Solidly fork on Base).
pub const AERODROME_V1_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0xca, 0xc8, 0x8e, 0xa9],
            protocol_scope: Some("aerodrome.v1"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Swap,
            semi_adapter: SemiAdapterId::AerodromeV1Swap,
            confidence: Confidence::High,
            promote: true,
            notes: "swapExactTokensForTokens (Solidly Route[])",
        },
    ),
];

/// Aave V3 Pool (이자 핵심 4종).
pub const AAVE_V3_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0x61, 0x7b, 0xa0, 0x37],
            protocol_scope: Some("aave.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Lending,
            action_type: ActionType::Supply,
            semi_adapter: SemiAdapterId::AaveV3Lending,
            confidence: Confidence::High,
            promote: true,
            notes: "supply(asset, amount, onBehalfOf, referralCode)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x69, 0x32, 0x8d, 0xec],
            protocol_scope: Some("aave.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Lending,
            action_type: ActionType::WithdrawCollateral,
            semi_adapter: SemiAdapterId::AaveV3Lending,
            confidence: Confidence::High,
            promote: true,
            notes: "withdraw(asset, amount, to)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0xa4, 0x15, 0xbc, 0xad],
            protocol_scope: Some("aave.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Lending,
            action_type: ActionType::Borrow,
            semi_adapter: SemiAdapterId::AaveV3Lending,
            confidence: Confidence::High,
            promote: true,
            notes: "borrow(asset, amount, interestRateMode, referralCode, onBehalfOf)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x57, 0x3a, 0xde, 0x81],
            protocol_scope: Some("aave.v3"),
        },
        DispatchEntry {
            category: ActionCategory::Lending,
            action_type: ActionType::Repay,
            semi_adapter: SemiAdapterId::AaveV3Lending,
            confidence: Confidence::High,
            promote: true,
            notes: "repay(asset, amount, rateMode, onBehalfOf)",
        },
    ),
];

/// Morpho Blue (이자 4종 — supply만 제공).
pub const MORPHO_BLUE_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0x23, 0x8d, 0x65, 0x79],
            protocol_scope: Some("morpho.blue"),
        },
        DispatchEntry {
            category: ActionCategory::Lending,
            action_type: ActionType::Supply,
            semi_adapter: SemiAdapterId::MorphoBlueLending,
            confidence: Confidence::High,
            promote: true,
            notes: "supply(marketParams, assets, shares, onBehalf, data)",
        },
    ),
];

/// Lido — submit / requestWithdrawals + wstETH wrap/unwrap.
pub const LIDO_TABLE: &[(DispatchKey, DispatchEntry)] = &[
    (
        DispatchKey::Selector {
            selector: [0xa1, 0x90, 0x3e, 0xab],
            protocol_scope: Some("lido"),
        },
        DispatchEntry {
            category: ActionCategory::LiquidStaking,
            action_type: ActionType::Stake,
            semi_adapter: SemiAdapterId::LidoStake,
            confidence: Confidence::High,
            promote: true,
            notes: "submit(referral) payable",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0x55, 0xed, 0x4a, 0xd9],
            protocol_scope: Some("lido"),
        },
        DispatchEntry {
            category: ActionCategory::LiquidStaking,
            action_type: ActionType::UnstakeRequest,
            semi_adapter: SemiAdapterId::LidoUnstakeRequest,
            confidence: Confidence::High,
            promote: true,
            notes: "requestWithdrawals(amounts[], owner)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0xea, 0x59, 0x8c, 0xb0],
            protocol_scope: Some("lido"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Wrap,
            semi_adapter: SemiAdapterId::LidoWstethWrap,
            confidence: Confidence::High,
            promote: true,
            notes: "wstETH.wrap(stETHAmount)",
        },
    ),
    (
        DispatchKey::Selector {
            selector: [0xde, 0x0e, 0x9a, 0x3e],
            protocol_scope: Some("lido"),
        },
        DispatchEntry {
            category: ActionCategory::Swap,
            action_type: ActionType::Unwrap,
            semi_adapter: SemiAdapterId::LidoWstethUnwrap,
            confidence: Confidence::High,
            promote: true,
            notes: "wstETH.unwrap(wstETHAmount)",
        },
    ),
];

/// 모든 sub-table을 합한 plan-time view.
pub const ALL_TABLES: &[&[(DispatchKey, DispatchEntry)]] = &[
    UNISWAP_V2_TABLE,
    UNISWAP_V3_TABLE,
    UNISWAP_UR_SELECTOR_TABLE,
    AERODROME_V1_TABLE,
    AAVE_V3_TABLE,
    MORPHO_BLUE_TABLE,
    LIDO_TABLE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tables_have_unique_selectors_within_protocol() {
        for table in ALL_TABLES {
            let mut seen = Vec::new();
            for (key, _) in *table {
                if let DispatchKey::Selector { selector, protocol_scope } = key {
                    let id = (*selector, *protocol_scope);
                    assert!(
                        !seen.contains(&id),
                        "중복 selector·scope: {selector:?}"
                    );
                    seen.push(id);
                }
            }
        }
    }

    #[test]
    fn entry_count() {
        // 9 + 4 + 2 + 1 + 4 + 1 + 4 = 25
        let total: usize = ALL_TABLES.iter().map(|t| t.len()).sum();
        assert_eq!(total, 25);
    }

    #[test]
    fn ur_promote_false() {
        // RouterPlan은 자식 컨테이너이므로 promote=false
        for (_, entry) in UNISWAP_UR_SELECTOR_TABLE {
            assert!(!entry.promote);
            assert_eq!(entry.action_type, ActionType::RouterPlan);
        }
    }
}
