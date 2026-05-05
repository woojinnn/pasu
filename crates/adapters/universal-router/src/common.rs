//! Shared token/address helpers for the Universal Router adapter.

use policy_engine::prelude::*;
use std::collections::HashMap;

/// Uniswap Universal Router v2.0 on Ethereum mainnet.
pub const UNIVERSAL_ROUTER_MAINNET: &str = "0x66a9893cc07d91d95644aedd05d03f95e1dba8af";

/// Sentinel address used by the policy engine for native ETH.
pub const NATIVE_ETH_SENTINEL: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

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
        me.add(native_eth(1));
        me
    }

    pub fn add(&mut self, token: Token) {
        self.tokens
            .insert((token.chain_id, token.address.0.to_lowercase()), token);
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

pub fn native_eth(chain_id: ChainId) -> Token {
    Token {
        chain_id,
        address: Address::new(NATIVE_ETH_SENTINEL).unwrap(),
        symbol: "ETH".into(),
        decimals: 18,
        is_native: true,
    }
}

pub fn currency_to_policy_address(currency: alloy_primitives::Address) -> Address {
    if currency == alloy_primitives::Address::ZERO {
        Address::new(NATIVE_ETH_SENTINEL).unwrap()
    } else {
        Address::from_alloy(currency)
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
