use serde::{Deserialize, Serialize};

/// CAIP-2 형식의 체인 식별자. 예: "eip155:1", "eip155:42161", "solana:..." 등.
///
/// EVM L1/L2 외 비-EVM 체인도 같은 타입으로 표현 가능.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChainId(pub String);

impl ChainId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn ethereum_mainnet() -> Self {
        Self("eip155:1".into())
    }

    pub fn arbitrum() -> Self {
        Self("eip155:42161".into())
    }

    pub fn base() -> Self {
        Self("eip155:8453".into())
    }
}

impl From<&str> for ChainId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for ChainId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 한 체인의 sync 시점 블록 정보.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockHeight {
    pub number: u64,
    pub time: u64,
}
