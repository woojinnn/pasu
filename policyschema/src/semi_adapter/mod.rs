//! Decoder 모듈 — calldata args (이미 ABI로 디코드된 JSON) → `ActionFields`
//! 변환 함수들의 컬렉션.
//!
//! 각 프로토콜 sub-module은 `FieldBuilder` 구현체들을 제공하고, `dispatch.rs`가
//! `(selector, opcode, primaryType)` → 어느 decoder를 호출할지 라우팅한다.
//!
//! # 패턴 (liam191/scopeball-test 차용)
//!
//! ```text
//! decode(args) → DecodedParams
//!   ↓
//! build(params, ctx) → ActionFields  (Token registry lookup, deadline horizon 계산 등)
//!   ↓
//! validate (round-trip 테스트로 검증)
//! ```

pub mod classify;
pub mod common;
pub mod error;
pub mod registry;

// 프로토콜별 decoder
pub mod uniswap_v2;
pub mod uniswap_v3;
pub mod uniswap_ur;
pub mod uniswap_v4;
pub mod pancakeswap;
pub mod aerodrome_v1;
pub mod aerodrome_slipstream;
pub mod aave_v3;
pub mod morpho_blue;
pub mod lido;
pub mod sign;

pub use error::SemiAdapterError;
pub use registry::{token_metadata, ur_family_for, mask_for, UrFamily};

use serde_json::Value;

use crate::target::ContractTarget;
use crate::types::{Address, ChainId};

/// `FieldBuilder` 구현체에 전달되는 컨텍스트.
///
/// liam191에서 차용한 `block_timestamp: Option<u64>` — `None`이면
/// `deadline_horizon_seconds`도 `None`. v0.2 prod에서는 RPC를 통해 채움.
#[derive(Debug, Clone)]
pub struct BuildContext<'a> {
    pub chain_id: ChainId,
    pub actor: Address,
    /// 호출 대상 컨트랙트 (`tx.to` 또는 자식 호출의 target).
    pub target: Address,
    /// `tx.value`를 십진 문자열로 보존.
    pub value_wei: String,
    /// 디코드 시점의 block.timestamp. 없으면 `None`.
    pub block_timestamp: Option<u64>,
    /// 관련 target 카탈로그 (token / pool / hook).
    pub targets: &'a [ContractTarget],
}

/// 모든 프로토콜 decoder가 구현하는 trait.
///
/// `Output`은 보통 특정 ActionFields variant 또는 그 안의 sub-struct.
pub trait FieldBuilder {
    type Output;
    fn decode(&self, args: &Value, ctx: &BuildContext) -> Result<Self::Output, SemiAdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::Confidence;
    use crate::target::{DiscoveredBy, TargetRole, Verification};

    #[test]
    fn decode_context_construct() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let target: Address = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
            .parse()
            .unwrap();
        let _t = ContractTarget {
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
        };
        let targets = vec![_t];
        let ctx = BuildContext {
            chain_id: 1,
            actor,
            target,
            value_wei: "0".into(),
            block_timestamp: Some(1_700_000_000),
            targets: &targets,
        };
        assert_eq!(ctx.chain_id, 1);
        assert_eq!(ctx.actor, actor);
    }
}
