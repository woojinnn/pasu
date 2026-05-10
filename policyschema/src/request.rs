//! `Request` — `TransactionRequest` (eth_sendTransaction)와 `TypedDataRequest`
//! (eth_signTypedData_v4)의 최상위 union.
//!
//! x-source: action-derived (원본 provider 요청).

use serde::{Deserialize, Serialize};

use crate::types::{Address, ChainId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    Transaction(TransactionRequest),
    TypedData(TypedDataRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionRequest {
    pub method: String,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub from: Address,
    pub to: Address,
    /// uint256 값을 십진 문자열로 보존 (전체 범위 보존, JS number drift 회피).
    pub value: String,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedDataRequest {
    pub method: String,
    pub signer: Address,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub domain: Eip712Domain,
    #[serde(rename = "primaryType")]
    pub primary_type: String,
    /// 원본 typed-data 메시지 (string-keyed JSON 객체).
    pub message: serde_json::Value,
    /// EIP-712 type 정의 JSON (`{ "PermitSingle": [...], ... }`).
    pub types: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Eip712Domain {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<ChainId>,
    #[serde(rename = "verifyingContract", skip_serializing_if = "Option::is_none")]
    pub verifying_contract: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub salt: Option<String>,
}
