use alloy_primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};

pub type ProviderName = String;

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
    #[must_use]
    pub const fn latest() -> Self {
        Self::Latest
    }

    #[must_use]
    pub fn as_param(&self) -> String {
        match self {
            Self::Latest => "latest".into(),
            Self::Pending => "pending".into(),
            Self::Earliest => "earliest".into(),
            Self::Number(n) => format!("0x{n:x}"),
        }
    }
}
