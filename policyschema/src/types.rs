//! 모든 `ActionFields`가 재사용하는 공유 타입.
//!
//! JSON Schema 참조 디자인의 `$defs` 패턴을 미러링: 각 타입을 한 번만 정의하고
//! 합성으로 embed.
//!
//! 규약
//! ----
//! - 주소는 `alloy_primitives::Address`이며 `0x` mixed-case checksum으로 직렬화.
//! - `uint256` 값은 **십진 문자열**로 직렬화 (JS number drift 회피, U256 전체
//!   범위 보존).
//! - 모든 필드의 doc comment에 `x-source` 메타가 있어 값의 출처를 명시
//!   (`action-derived` | `adapter:metadata` | `derived`).

use serde::{Deserialize, Serialize};

pub type ChainId = u64;
pub type Address = alloy_primitives::Address;

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/// `chainId × address × symbol × decimals × isNative` — 최소 토큰 메타데이터.
///
/// x-source:
///   - `chain_id`/`address`: action-derived (path / struct에서 추출)
///   - `symbol`/`decimals`: adapter:metadata (큐레이트 레지스트리 lookup)
///   - `is_native`: action-derived (프로토콜별 sentinel `0xeeee…` 또는 `address == 0x0`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
    /// 체인 native 자산을 의미하면 `true` (mainnet의 ETH, BSC의 BNB 등).
    /// `true`인 경우 `address`는 sentinel.
    #[serde(rename = "isNative")]
    pub is_native: bool,
}

// ---------------------------------------------------------------------------
// AmountSpec
// ---------------------------------------------------------------------------

/// `uint256` raw 값 + 의미 분류 태그.
///
/// `raw`는 십진 문자열로 보존(decimals 미적용)하여 정책이 손실 변환 없이
/// 정확히 비교 가능. `kind`는 "정확히 N" / "최소 N" / "최대 N" / "무제한"을 구분.
///
/// x-source: action-derived
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountSpec {
    /// uint256 십진 문자열 (decimals 미적용 — 온체인 raw 값 그대로).
    pub raw: String,
    pub kind: AmountKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmountKind {
    /// 정확한 양 (예: `swapExactTokensForTokens.amountIn`, EIP-2612 `value`).
    Exact,
    /// 최소 보장 (예: `amountOutMin`).
    Min,
    /// 최대 한도 (예: `amountInMax`).
    Max,
    /// `type(uint256).max` / `type(uint160).max` sentinel — 무제한.
    /// Permit2 unlimited approval, Aave repay-max 등에 흔함.
    Unlimited,
    /// 정적으로 결정 불가 (event-derived, callback 등).
    Unspecified,
}

impl AmountSpec {
    /// 십진 문자열 raw가 주어진 정확한 amount 편의 생성자.
    pub fn exact(raw: impl Into<String>) -> Self {
        Self { raw: raw.into(), kind: AmountKind::Exact }
    }

    /// 최소 amount 편의 생성자.
    pub fn min(raw: impl Into<String>) -> Self {
        Self { raw: raw.into(), kind: AmountKind::Min }
    }

    /// 최대 amount 편의 생성자.
    pub fn max(raw: impl Into<String>) -> Self {
        Self { raw: raw.into(), kind: AmountKind::Max }
    }
}

// ---------------------------------------------------------------------------
// PoolKey (Uniswap V4 / Aerodrome Slipstream)
// ---------------------------------------------------------------------------

/// V4-style 풀 식별자 — `uniswap.v4`에서 사용. Aerodrome Slipstream도 비슷한
/// 모양 (단, `hooks` 없음). 그곳에서는 이 struct를 그대로 재사용하되 `hooks`를
/// zero address로 둔다.
///
/// x-source: adapter:metadata (V4 PoolManager `unlock` 콜백 또는 action params에서 디코드)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolKey {
    /// 정렬 시 작은 currency. `0x0`은 V4의 native ETH.
    pub currency0: Address,
    /// 정렬 시 큰 currency.
    pub currency1: Address,
    /// `uint24` 패킹 fee. 최상위 bit (`0x800000`)는 dynamic-fee marker;
    /// 호출자는 effective bps 계산 시 마스킹 필요.
    pub fee: u32,
    #[serde(rename = "tickSpacing")]
    pub tick_spacing: i32,
    /// hook 컨트랙트; `0x0` = no hook.
    pub hooks: Address,
}

// ---------------------------------------------------------------------------
// RecipientFields fragment
// ---------------------------------------------------------------------------

/// `recipient` / `to` / `receiver` 인자가 있는 모든 곳에서 `SwapFields`,
/// `LendingFields`, `StakingFields`가 embed.
///
/// `recipient_equals_actor`와 `has_external_recipient`은 수학적으로 부정 관계.
/// 정책 언어별로 자연스러운 표현이 달라서 둘 다 surface한다 — 정규화기가 둘
/// 모두 채움.
///
/// x-source: action-derived (recipient 필드) + derived (boolean 플래그)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientFields {
    /// recipient가 암묵적이거나 (예: Lido `submit`은 `msg.sender`에 mint),
    /// action에 recipient 개념이 없으면 (예: `signPermit`) `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<RecipientRef>,
    /// derived: `recipient == actor`.
    #[serde(rename = "recipientEqualsActor")]
    pub recipient_equals_actor: bool,
    /// derived: `recipient_equals_actor`의 부정. 이미 사용 중인 정책의
    /// 가독성을 유지하기 위해 legacy DexFacts 명칭을 그대로 두었다.
    #[serde(rename = "hasExternalRecipient")]
    pub has_external_recipient: bool,
}

/// 출력 토큰이 가는 곳.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecipientRef {
    /// 구체적 EOA 또는 컨트랙트 주소.
    Address {
        address: Address,
    },
    /// PancakeSwap SmartRouter sentinel (`0x…01` = msg.sender, `0x…02` = router 자체).
    Sentinel {
        sentinel: String,
    },
    /// `actor`와 동일 (암묵적 `msg.sender`).
    Actor,
}

// ---------------------------------------------------------------------------
// DeadlineFields fragment
// ---------------------------------------------------------------------------

/// deadline을 갖는 모든 action(swap, signPermit*, requestWithdrawal 등)이 embed.
///
/// `deadline_horizon_seconds`는 정규화 시점에 `block.timestamp`를 알 때만
/// 채워지고, 그 외엔 `None`.
///
/// x-source: action-derived (deadline) + derived (horizon)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadlineFields {
    /// Unix timestamp (epoch 이후 초) — 프로토콜이 deadline 개념을 갖지 않으면 `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<u64>,
    /// derived: `deadline - block.timestamp`. 디코드 시점 기준으로 deadline이
    /// 이미 지났다면 음수.
    #[serde(rename = "deadlineHorizonSeconds", skip_serializing_if = "Option::is_none")]
    pub deadline_horizon_seconds: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_kind_serializes_pascal_case() {
        let json = serde_json::to_string(&AmountKind::Unlimited).unwrap();
        assert_eq!(json, "\"Unlimited\"");
    }

    #[test]
    fn amount_spec_round_trip() {
        let spec = AmountSpec::exact("1000000000");
        let json = serde_json::to_string(&spec).unwrap();
        let back: AmountSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn token_round_trip() {
        let t = Token {
            chain_id: 1,
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Token = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn recipient_ref_tagged() {
        let r = RecipientRef::Actor;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"kind":"actor"}"#);
    }
}
