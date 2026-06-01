//! Core domain types used by host capability traits.
//!
//! Includes `Address`, `Token`, `AmountSpec`, `UsdValuation`, and the EVM
//! transaction / signature request shapes consumed by host capabilities
//! (`oracle`, `portfolio`, `approvals`, `stat_windows`).
//!
//! The active action-model pipeline enters through
//! `simulation_reducer::action::ActionBody`; these types remain here because
//! the host capability traits speak in terms of `Token`/`AmountSpec` for
//! oracle valuation and portfolio accounting.
//!
//! Note: `action::Address` is a distinct lowercase-hex newtype used by the
//! action schema; `core::Address` is retained for host capability traits
//! that pre-date the envelope work.
//!
//! `Eip712TypedData` / `Eip712Domain` are kept here because `SignatureRequest`
//! references them; they're not part of any active lowering path.
//!
//! `validate_typed_data` has been removed alongside the legacy signature
//! adapter machinery.

use alloy_primitives::{Address as AlloyAddress, U256};
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

/// EVM address as a lowercase hex string with 0x prefix.
///
/// `Deserialize` is implemented manually to route through `Address::new`,
/// which normalizes the input to the same lowercase form that `from_alloy`
/// produces.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Address(String);

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(&raw).map_err(serde::de::Error::custom)
    }
}

impl Address {
    /// Parse and normalize an EVM address.
    ///
    /// # Errors
    ///
    /// Returns an error when `s` is not a valid 20-byte hex address.
    pub fn new(s: &str) -> Result<Self, String> {
        let parsed = AlloyAddress::from_str(s).map_err(|e| format!("invalid address: {e}"))?;
        Ok(Self(format!("{parsed:#x}")))
    }

    /// Convert an Alloy address into the engine address wrapper.
    #[must_use]
    pub fn from_alloy(a: AlloyAddress) -> Self {
        Self(format!("{a:#x}"))
    }

    /// Borrow the normalized lowercase hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Engine chain id.
pub type ChainId = u64;

/// Token metadata as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// EVM chain id this token belongs to.
    pub chain_id: ChainId,
    /// Token contract address or native sentinel address.
    pub address: Address,
    /// Human-readable token symbol.
    pub symbol: String,
    /// Token decimal precision.
    pub decimals: u32,
    /// Whether this token represents the native asset.
    pub is_native: bool,
}

impl Token {
    /// Return the chain-qualified token key used by host capability maps.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.chain_id, self.address.as_str().to_lowercase())
    }
}

/// Provenance + value information about an oracle-derived USD valuation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsdValuation {
    /// USD value as a stringified decimal (e.g., "1234.56").
    pub value: String,
    /// Source timestamp for the valuation.
    pub as_of_ts: u64,
    /// Data sources that contributed to the valuation.
    pub sources: Vec<String>,
    /// Age of the valuation in seconds.
    pub stale_sec: u64,
}

/// Amount paired with its token, with optional human-readable and USD views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountSpec {
    /// Token this amount is denominated in.
    pub token: Token,
    /// Wei-scale raw amount as decimal string (so we can carry full U256 range).
    pub raw: String,
    /// Optional `raw / 10^decimals` representation as a decimal string.
    pub human: Option<String>,
    /// Optional USD valuation; present when an oracle was available.
    pub usd: Option<UsdValuation>,
}

impl AmountSpec {
    /// Construct an amount from a raw integer value.
    #[must_use]
    pub fn from_raw(token: Token, raw: U256) -> Self {
        Self {
            token,
            raw: raw.to_string(),
            human: None,
            usd: None,
        }
    }
}

/// Unsigned transaction request presented to the policy engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRequest {
    /// EVM chain id.
    pub chain_id: ChainId,
    /// Sender wallet address.
    pub from: Address,
    /// Target contract address.
    pub to: Address,
    /// `msg.value` in wei, decimal-encoded so we can carry a full U256.
    pub value_wei: String,
    /// Calldata (function selector + ABI-encoded args).
    pub data: Vec<u8>,
    /// Gas limit, when known.
    pub gas: Option<u64>,
    /// Account nonce, when known.
    pub nonce: Option<u64>,
}

impl TransactionRequest {
    /// First four bytes of `data`, if present, as the ABI function selector.
    #[must_use]
    pub fn selector(&self) -> Option<[u8; 4]> {
        if self.data.len() < 4 {
            return None;
        }
        Some([self.data[0], self.data[1], self.data[2], self.data[3]])
    }

    /// First four bytes of `data`, if present, as `0x`-prefixed hex.
    #[must_use]
    pub fn selector_hex(&self) -> Option<String> {
        self.selector().map(|s| format!("0x{}", hex::encode(s)))
    }
}

/// Off-chain EIP-712 signature request presented to the policy engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureRequest {
    /// EVM chain id selected by the wallet request.
    pub chain_id: ChainId,
    /// Wallet that is being asked to sign.
    pub signer: Address,
    /// Typed data payload.
    pub typed_data: Eip712TypedData,
}

impl SignatureRequest {
    /// Borrow the EIP-712 primary type.
    #[must_use]
    pub fn primary_type(&self) -> &str {
        &self.typed_data.primary_type
    }
}

/// EIP-712 typed-data payload.
// `serde_json::Value` carries an f64 number variant, so `Eq` is intentionally
// not derived.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip712TypedData {
    /// EIP-712 domain.
    pub domain: Eip712Domain,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// EIP-712 type map.
    pub types: serde_json::Value,
    /// EIP-712 message object.
    pub message: serde_json::Value,
}

/// EIP-712 domain fields used by v1 signature policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip712Domain {
    /// Optional domain name.
    pub name: Option<String>,
    /// Optional domain version.
    pub version: Option<String>,
    /// EIP-712 domain chain id.
    pub chain_id: ChainId,
    /// EIP-712 verifying contract.
    pub verifying_contract: Address,
    /// Optional domain salt.
    pub salt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_normalizes_to_lowercase_hex() {
        let a = Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
        assert_eq!(a.as_str(), "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
    }

    #[test]
    fn address_rejects_garbage() {
        assert!(Address::new("not-an-address").is_err());
        assert!(Address::new("0x1234").is_err());
    }

    #[test]
    fn token_key_is_chain_qualified() {
        let t = Token {
            chain_id: 1,
            address: Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        };
        assert_eq!(t.key(), "1:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
    }

    #[test]
    fn tx_selector_is_first_four_bytes() {
        let mut data = vec![0x41, 0x4b, 0xf3, 0x89];
        data.extend_from_slice(&[0u8; 32]);
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            value_wei: "0".into(),
            data,
            gas: None,
            nonce: None,
        };
        assert_eq!(tx.selector_hex().unwrap(), "0x414bf389");
    }

    #[test]
    fn tx_selector_returns_none_for_short_data() {
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            value_wei: "0".into(),
            data: vec![0x41, 0x4b],
            gas: None,
            nonce: None,
        };
        assert!(tx.selector_hex().is_none());
    }

    #[test]
    fn tx_carries_optional_gas_and_nonce() {
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            value_wei: "0".into(),
            data: vec![0x00, 0x01, 0x02, 0x03],
            gas: Some(200_000),
            nonce: Some(42),
        };
        assert_eq!(tx.gas, Some(200_000));
        assert_eq!(tx.nonce, Some(42));
    }
}
