//! `ContractTarget` — calldata가 거치는 모든 컨트랙트마다 1개 entry.
//! 각 target은 `role`(router | pool | token | ...)과 `discoveredBy`(정규화기가
//! 어떻게 찾았는지)를 갖는다.
//!
//! x-source: action-derived (tx.to + 디코드된 인자에서 target 목록 조립).

use serde::{Deserialize, Serialize};

use crate::confidence::Confidence;
use crate::types::{Address, ChainId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractTarget {
    /// cross-reference용 안정 id (예: `t#router`).
    pub id: String,
    pub address: Address,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub role: TargetRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProtocolRef>,
    #[serde(rename = "discoveredBy")]
    pub discovered_by: DiscoveredBy,
    pub verification: Verification,
    pub confidence: Confidence,
}

/// 이 target이 어떤 프로토콜(과 component)을 의미하는지의 reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 예: `"router02"`, `"poolManager"`, `"withdrawalQueue"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRole {
    Entrypoint,
    Router,
    Aggregator,
    Pool,
    Vault,
    Manager,
    Token,
    Permit,
    Hook,
    Account,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveredBy {
    /// 직접 `tx.to`.
    TxTo,
    /// calldata 인자에서 디코드 (예: recipient).
    CalldataArg,
    /// `path` 배열 / encoded path bytes에서 디코드.
    PathDecode,
    /// Balancer-style `bytes32 poolId`에서 디코드.
    PoolIdDecode,
    /// 큐레이트된 registry에서 lookup.
    ManualRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// `"curated"` | `"etherscan"` | `"unknown"`.
    #[serde(rename = "labelSource")]
    pub label_source: String,
    /// 이 주소에 검증된 ABI가 있으면 `true`.
    #[serde(rename = "abiAvailable")]
    pub abi_available: bool,
    /// 온체인 소스코드가 검증되어 있으면 `true`.
    #[serde(rename = "contractVerified")]
    pub contract_verified: bool,
    /// 프록시가 구현체로 resolve되었으면 `true`.
    #[serde(rename = "proxyResolved", skip_serializing_if = "Option::is_none")]
    pub proxy_resolved: Option<bool>,
}
