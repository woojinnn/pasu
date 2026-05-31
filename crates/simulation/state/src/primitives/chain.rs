use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// Chain identifier in CAIP-2 format. Examples: "eip155:1", "eip155:42161", "solana:...".
///
/// Non-EVM chains beyond EVM L1/L2 can be represented with the same type.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
pub struct ChainId(pub String);

impl ChainId {
    /// Creates a `ChainId` from any value convertible into a `String` (e.g. a CAIP-2 string).
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Returns the underlying CAIP-2 identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the `ChainId` for Ethereum mainnet ("eip155:1").
    #[must_use]
    pub fn ethereum_mainnet() -> Self {
        Self("eip155:1".into())
    }

    /// Returns the `ChainId` for Arbitrum One ("eip155:42161").
    #[must_use]
    pub fn arbitrum() -> Self {
        Self("eip155:42161".into())
    }

    /// Returns the `ChainId` for Base ("eip155:8453").
    #[must_use]
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

/// Block information for a chain at sync time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BlockHeight {
    /// Block number (height) at the moment of sync.
    pub number: u64,
    /// Block timestamp in Unix seconds.
    pub time: u64,
}
