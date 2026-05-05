//! Shared resources for V2 Router02 swap-function adapters.

use policy_engine::prelude::*;
use std::collections::HashMap;

/// Uniswap V2 Router02 on mainnet.
pub const UNISWAP_V2_ROUTER_MAINNET: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

/// Sentinel address used to represent native ETH inside our `Token` model.
/// Not the same as any deployed token contract — it's purely an identifier.
pub const NATIVE_ETH_SENTINEL: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

/// Construct a `Token` representing native ETH on the given chain.
pub fn native_eth(chain_id: ChainId) -> Token {
    Token {
        chain_id,
        address: Address::new(NATIVE_ETH_SENTINEL).unwrap(),
        symbol: "ETH".into(),
        decimals: 18,
        is_native: true,
    }
}

pub struct TokenLookup {
    tokens: HashMap<(ChainId, String), Token>,
}

impl TokenLookup {
    pub fn with_mainnet_defaults() -> Self {
        let mut me = TokenLookup {
            tokens: HashMap::new(),
        };
        me.add(Token {
            chain_id: 1,
            address: Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            symbol: "USDT".into(),
            decimals: 6,
            is_native: false,
        });
        me.add(Token {
            chain_id: 1,
            address: Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        });
        me.add(Token {
            chain_id: 1,
            address: Address::new("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap(),
            symbol: "WETH".into(),
            decimals: 18,
            is_native: false,
        });
        me
    }

    pub fn add(&mut self, token: Token) {
        self.tokens
            .insert((token.chain_id, token.address.0.to_lowercase()), token);
    }

    pub fn with(mut self, token: Token) -> Self {
        self.add(token);
        self
    }

    pub fn get(&self, chain_id: ChainId, addr: &Address) -> Token {
        self.tokens
            .get(&(chain_id, addr.0.to_lowercase()))
            .cloned()
            .unwrap_or_else(|| Token {
                chain_id,
                address: addr.clone(),
                symbol: "UNKNOWN".into(),
                decimals: 18,
                is_native: false,
            })
    }
}

impl Default for TokenLookup {
    fn default() -> Self {
        Self::with_mainnet_defaults()
    }
}

pub fn shift_decimals(value: &str, decimals: u32) -> String {
    if decimals == 0 {
        return value.to_string();
    }
    let pad_len = decimals as usize;
    let padded = if value.len() <= pad_len {
        format!("{}{}", "0".repeat(pad_len + 1 - value.len()), value)
    } else {
        value.to_string()
    };
    let split_at = padded.len() - pad_len;
    let (whole, frac) = padded.split_at(split_at);
    format!("{whole}.{frac}")
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("calldata too short: need at least {need} bytes, got {got}")]
    TooShort { need: usize, got: usize },
    #[error("unexpected selector: got 0x{got}, expected 0x{want}")]
    BadSelector { got: String, want: String },
    #[error("ABI decode failed: {0}")]
    AbiDecode(String),
    #[error("path must contain at least 2 tokens, got {0}")]
    EmptyPath(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_eth_marks_is_native() {
        let n = native_eth(1);
        assert!(n.is_native);
        assert_eq!(n.symbol, "ETH");
        assert_eq!(n.decimals, 18);
    }

    #[test]
    fn token_lookup_returns_known_tokens() {
        let lookup = TokenLookup::with_mainnet_defaults();
        let weth = Address::new("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap();
        assert_eq!(lookup.get(1, &weth).symbol, "WETH");
    }

    #[test]
    fn token_lookup_unknown_falls_back() {
        let lookup = TokenLookup::with_mainnet_defaults();
        let unknown = Address::new("0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap();
        assert_eq!(lookup.get(1, &unknown).symbol, "UNKNOWN");
    }

    #[test]
    fn shift_decimals_basic() {
        assert_eq!(shift_decimals("1000000", 6), "1.000000");
    }
}
