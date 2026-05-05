//! Core domain types: Address, Token, AmountSpec, Action.

use alloy_primitives::{Address as AlloyAddress, U256};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// EVM address as a lowercase hex string with 0x prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub String);

impl Address {
    pub fn new(s: &str) -> Result<Self, String> {
        let parsed = AlloyAddress::from_str(s).map_err(|e| format!("invalid address: {e}"))?;
        Ok(Address(format!("{parsed:#x}")))
    }

    pub fn from_alloy(a: AlloyAddress) -> Self {
        Address(format!("{a:#x}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Chain id (EIP-155).
pub type ChainId = u64;

/// Token metadata as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub chain_id: ChainId,
    pub address: Address,
    pub symbol: String,
    pub decimals: u32,
    pub is_native: bool,
}

impl Token {
    pub fn key(&self) -> String {
        format!("{}:{}", self.chain_id, self.address.0.to_lowercase())
    }
}

/// Provenance + value information about an oracle-derived USD valuation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsdValuation {
    /// USD value as a stringified decimal (e.g., "1234.56").
    /// We use a string here because Cedar's `Decimal` extension is the place
    /// that actually does the comparison and we want to avoid f64 drift.
    pub value: String,
    pub as_of_ts: u64,
    pub sources: Vec<String>,
    pub stale_sec: u64,
}

/// Amount paired with its token, with optional human-readable and USD views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmountSpec {
    pub token: Token,
    /// Wei-scale raw amount as decimal string (so we can carry full U256 range).
    pub raw: String,
    /// Optional `raw / 10^decimals` representation as a decimal string.
    pub human: Option<String>,
    /// Optional USD valuation; present when an oracle was available.
    pub usd: Option<UsdValuation>,
}

impl AmountSpec {
    pub fn from_raw(token: Token, raw: U256) -> Self {
        AmountSpec {
            token,
            raw: raw.to_string(),
            human: None,
            usd: None,
        }
    }
}

/// A `swap` semantic action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwapAction {
    pub protocol_id: String,
    pub actor: Address,
    pub target: Address,
    pub value_wei: String,
    pub input_token: Token,
    pub output_token: Token,
    pub input_amount: AmountSpec,
    pub min_output_amount: Option<AmountSpec>,
    pub recipient: Address,
    pub deadline: Option<u64>,
    pub fee_bips: Option<u32>,
}

/// A composite transaction action. Policy evaluation normally expands this
/// into leaf actions so existing `swap` policies continue to apply unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiAction {
    pub actor: Address,
    pub target: Address,
    pub value_wei: String,
    pub children: Vec<Action>,
}

/// Semantic action emitted by adapters.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Swap(SwapAction),
    Multi(MultiAction),
    Other {
        actor: Address,
        target: Address,
        selector: String,
        value_wei: String,
        raw_calldata: String,
    },
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Swap(_) => "swap",
            Action::Multi(_) => "multi",
            Action::Other { .. } => "other",
        }
    }

    pub fn target(&self) -> &Address {
        match self {
            Action::Swap(s) => &s.target,
            Action::Multi(m) => &m.target,
            Action::Other { target, .. } => target,
        }
    }

    pub fn actor(&self) -> &Address {
        match self {
            Action::Swap(s) => &s.actor,
            Action::Multi(m) => &m.actor,
            Action::Other { actor, .. } => actor,
        }
    }
}

/// What a wallet receives from a dapp/RPC layer when the user is asked to
/// sign a transaction. This is the unsigned-tx shape (the wallet has not yet
/// produced a signature). Fields that v0.1 policies don't reference yet
/// (`gas`, `nonce`, EIP-1559 fee fields) are kept as `Option` so future
/// policies (e.g., "reject txs with gas above X") can read them without
/// breaking existing call sites.
///
/// Naming aligns with `alloy::TransactionRequest` and `ethers::TransactionRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionRequest {
    pub chain_id: ChainId,
    pub from: Address,
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
    /// First four bytes of `data`, if present — the ABI function selector.
    pub fn selector(&self) -> Option<[u8; 4]> {
        if self.data.len() < 4 {
            return None;
        }
        Some([self.data[0], self.data[1], self.data[2], self.data[3]])
    }

    pub fn selector_hex(&self) -> Option<String> {
        self.selector().map(|s| format!("0x{}", hex::encode(s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_normalizes_to_lowercase_hex() {
        let a = Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
        assert_eq!(a.0, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
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

    #[test]
    fn action_swap_kind_string() {
        let usdc = Token {
            chain_id: 1,
            address: Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        };
        let weth = Token {
            chain_id: 1,
            address: Address::new("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap(),
            symbol: "WETH".into(),
            decimals: 18,
            is_native: false,
        };
        let zero = Address::new("0x0000000000000000000000000000000000000000").unwrap();
        let act = Action::Swap(SwapAction {
            protocol_id: "uniswap-v3".into(),
            actor: zero.clone(),
            target: zero.clone(),
            value_wei: "0".into(),
            input_token: usdc.clone(),
            output_token: weth.clone(),
            input_amount: AmountSpec::from_raw(usdc, U256::from(1u64)),
            min_output_amount: None,
            recipient: zero,
            deadline: None,
            fee_bips: Some(30),
        });
        assert_eq!(act.kind(), "swap");
    }
}
