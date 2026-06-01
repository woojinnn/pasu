//! Sourcify 통합 (스켈레톤).
//!
//! 추후 작업: 컨트랙트 주소 + 함수 이름으로 Sourcify 에 verified ABI 조회 →
//! 자동으로 `DynSolType` 생성 + 캐싱.
//!
//! 지금은 stub — 등록된 builtin ABI 로 충분한 동안은 사용 안 함.
//! 새 protocol 자동화 시 이 모듈을 채워서:
//!   1. <https://repo.sourcify.dev/contracts/full_match/{chain}/{addr}/metadata.json>
//!   2. metadata.json 의 ABI 추출
//!   3. 원하는 함수의 outputs → `DynSolType` 변환
//!   4. `AbiTypeRegistry` 에 동적 register

use crate::error::SyncError;

/// Stub. 후속 패스에서 reqwest + sourcify URL 패턴으로 구현.
#[allow(dead_code, clippy::unused_async)]
pub async fn fetch_function_output_type(
    _chain_id: u64,
    _contract: alloy_primitives::Address,
    _function_name: &str,
) -> Result<alloy_dyn_abi::DynSolType, SyncError> {
    Err(SyncError::FetchFailed {
        source_id: "sourcify".into(),
        reason: "Sourcify auto-fetch not implemented yet — use AbiTypeRegistry::with_builtins() / register()".into(),
    })
}
