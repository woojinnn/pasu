//! Schema common primitives — mirrors `schema_demo/schema/common/_common.json`.
//!
//! These types are the canonical Rust representation of the JSON schema.
//! `serde` derives produce JSON that validates against the schema.

use serde::{Deserialize, Serialize};

/// EVM address as lowercase `0x`-prefixed hex.
pub type Address = String;

/// `uint256` rendered in decimal (avoids JSON number range issues).
pub type DecimalString = String;

/// `int256` rendered in decimal (allows negative).
pub type IntDecimalString = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Native,
    Erc20,
    Erc721,
    Erc1155,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    pub kind: AssetKind,
    pub chain_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmountKind {
    Exact,
    Min,
    Max,
    Unlimited,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountConstraint {
    pub kind: AmountKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<DecimalString>,
}

impl AmountConstraint {
    pub fn exact(v: impl Into<DecimalString>) -> Self {
        Self {
            kind: AmountKind::Exact,
            value: Some(v.into()),
        }
    }
    pub fn min(v: impl Into<DecimalString>) -> Self {
        Self {
            kind: AmountKind::Min,
            value: Some(v.into()),
        }
    }
    pub fn max(v: impl Into<DecimalString>) -> Self {
        Self {
            kind: AmountKind::Max,
            value: Some(v.into()),
        }
    }
    pub fn unlimited() -> Self {
        Self {
            kind: AmountKind::Unlimited,
            value: None,
        }
    }
    pub fn unknown() -> Self {
        Self {
            kind: AmountKind::Unknown,
            value: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsdValuation {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_ts: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_sec: Option<u64>,
}
