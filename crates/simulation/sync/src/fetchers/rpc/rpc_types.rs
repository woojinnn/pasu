//! RPC 호출의 입력/출력 보조 타입.

use alloy_primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};

/// provider 식별자 (config 의 name 필드와 일치).
pub type ProviderName = String;

/// `eth_call` 의 입력.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EthCallRequest {
    pub to: Address,
    pub data: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<U256>,
    #[serde(default = "BlockTag::latest")]
    pub block: BlockTag,
}

impl EthCallRequest {
    pub fn new(to: Address, data: impl Into<Bytes>) -> Self {
        Self {
            to,
            data: data.into(),
            from: None,
            value: None,
            block: BlockTag::Latest,
        }
    }
}

/// `block` 파라미터 — `latest` / `pending` / 구체 번호.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockTag {
    Latest,
    Pending,
    Earliest,
    #[serde(untagged)]
    Number(u64),
}

impl BlockTag {
    pub fn latest() -> Self {
        Self::Latest
    }

    /// JSON-RPC 의 block parameter 문자열.
    pub fn as_param(&self) -> String {
        match self {
            Self::Latest => "latest".into(),
            Self::Pending => "pending".into(),
            Self::Earliest => "earliest".into(),
            Self::Number(n) => format!("0x{:x}", n),
        }
    }
}
