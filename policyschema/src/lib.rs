//! `policyschema` — Cedar 정책 엔진 직전 단계의 공통 Action 데이터 모델.
//!
//! EVM 트랜잭션과 EIP-712 서명을 정책 레이어에 도달하기 전에 의미 단위
//! `Action`으로 정규화한다. 여러 DEX·렌딩·LST 프로토콜이 공통 표면(코어
//! `ActionFields`)을 공유하고, 프로토콜 특수 데이터는 namespace 키 기반의
//! `extensions[]`에 담는다.
//!
//! 설계 계약은 `docs/baseline.md` 참조.

pub mod action;
pub mod call;
pub mod confidence;
pub mod core;
pub mod semi_adapter;
pub mod dispatch;
pub mod extension;
pub mod raw;
pub mod request;
pub mod target;
pub mod types;

// 스키마 영역 — dispatch table 정의
pub use dispatch::{DispatchEntry, DispatchKey, SemiAdapterId, UrFamily};

// 세미-어댑터 영역 — 분류 실행 함수
pub use semi_adapter::classify::{
    classify_call, classify_slipstream, classify_v4_swap, ClassifyOutcome,
};

/// 셀렉터를 hex 문자열에서 4 byte로 파싱.
///
/// 세미-어댑터에서 자주 쓰여 lib 최상위에서 re-export.
pub fn parse_selector(s: &str) -> Result<[u8; 4], semi_adapter::error::SemiAdapterError> {
    let trimmed = s.trim_start_matches("0x");
    if trimmed.len() != 8 {
        return Err(semi_adapter::error::SemiAdapterError::BadSelector {
            expected: "8 hex chars".into(),
            got: s.into(),
        });
    }
    let bytes = hex::decode(trimmed)
        .map_err(|e| semi_adapter::error::SemiAdapterError::BadHex(e.to_string()))?;
    Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
}

pub use action::{Action, ActionCategory, ActionType};
pub use call::{CallType, DecodedCall, DecodeSource};
pub use confidence::{Confidence, ConfidenceReport, Stage};
pub use core::NormalizedRequestV2;
pub use extension::{Extension, ExtensionNamespace, ExtensionScope};
pub use raw::{Raw, RawTx, RawTypedData};
pub use request::{Eip712Domain, Request, TransactionRequest, TypedDataRequest};
pub use target::{ContractTarget, DiscoveredBy, TargetRole, Verification};
pub use types::{
    Address, AmountKind, AmountSpec, ChainId, DeadlineFields, PoolKey, RecipientFields,
    RecipientRef, Token,
};
