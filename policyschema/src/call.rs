//! `DecodedCall` — ABI로 디코드된 호출마다 1개 entry. 단일 트랜잭션이
//! Universal Router `execute(...)`, `multicall(bytes[])`, V4 batched action
//! list를 포함하면 다수 call이 생성된다.
//!
//! x-source: action-derived (calldata + ABI 디코드).

use serde::{Deserialize, Serialize};

use crate::confidence::Confidence;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedCall {
    /// 안정 id (예: `c#0`, 자식은 `c#1.ur.0`).
    pub id: String,
    /// 주 `ContractTarget.id` 참조.
    #[serde(rename = "targetId")]
    pub target_id: String,
    /// 이 호출이 건드리는 토큰·풀·hook target들.
    #[serde(rename = "relatedTargetIds", default)]
    pub related_target_ids: Vec<String>,
    #[serde(rename = "callType")]
    pub call_type: CallType,
    /// 4-byte 함수 셀렉터 (`0x` 헥스, 예: `0x38ed1739`).
    /// typed-data 서명 흐름에서는 `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(rename = "functionName", skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// canonical Solidity 시그니처 (예: `swapExactTokensForTokens(uint256,uint256,address[],address,uint256)`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 디코드된 인자(JSON 객체) — 함수별 적절한 모양은 어댑터가 결정.
    /// `uint256` 값은 십진 문자열.
    #[serde(default)]
    pub args: serde_json::Value,
    /// 최상위 호출의 `tx.value` 또는 sub-call value (십진 문자열).
    pub value: String,
    #[serde(rename = "decodeSource")]
    pub decode_source: DecodeSource,
    /// Universal Router 자식의 경우: 마스킹된 opcode (예: `0x00`).
    #[serde(rename = "urOpcode", skip_serializing_if = "Option::is_none")]
    pub ur_opcode: Option<u8>,
    /// Universal Router 자식의 경우: 어떤 family인지 (uniswap | pancakeswap).
    #[serde(rename = "urFamily", skip_serializing_if = "Option::is_none")]
    pub ur_family: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallType {
    /// EOA → 컨트랙트 직접 `eth_call`.
    ExternalCall,
    /// 정적 분석으로 발견한 내부 호출 (이 스키마에서는 드물다).
    InternalCall,
    /// Universal Router `execute(...)` 안의 1 opcode.
    RouterCommand,
    /// `multicall(bytes[])`의 1 원소.
    MulticallItem,
    /// V4 `modifyLiquidities` action list의 1 원소.
    BatchItem,
    /// 제어 흐름 패턴으로 식별된 hook 콜백.
    HookCallbackPattern,
    /// 이벤트 로그 모양에서만 추론(calldata 없음).
    LogInferred,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecodeSource {
    /// 검증된 소스의 ABI.
    VerifiedAbi,
    /// selector + 휴리스틱으로 추론한 ABI.
    InferredAbi,
    /// 부분 디코드 (일부 필드 미해결).
    PartialAbi,
    /// 이벤트 로그 + ABI로 재구성.
    EventLog,
    /// 바이트코드 수준 disassembly.
    Bytecode,
    Unknown,
}
