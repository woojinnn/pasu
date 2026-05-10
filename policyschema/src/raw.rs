//! `Raw` — 감사·재현용으로 보존된 원본 wire 데이터.

use serde::{Deserialize, Serialize};

use crate::types::{Address, ChainId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Raw {
    #[serde(rename = "providerRequest", skip_serializing_if = "Option::is_none")]
    pub provider_request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx: Option<RawTx>,
    #[serde(rename = "typedData", skip_serializing_if = "Option::is_none")]
    pub typed_data: Option<RawTypedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calldata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTx {
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub from: Address,
    pub to: Option<Address>,
    pub value: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTypedData {
    #[serde(rename = "providerJson")]
    pub provider_json: String,
}
