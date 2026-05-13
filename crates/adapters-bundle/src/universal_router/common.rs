//! Shared token/address helpers for the Universal Router adapter.

use alloy_primitives::address;
use policy_engine::prelude::*;
use std::collections::HashMap;

/// Uniswap Universal Router v2.0 on Ethereum mainnet.
pub const UNIVERSAL_ROUTER_MAINNET: &str = "0x66a9893cc07d91d95644aedd05d03f95e1dba8af";

/// Sentinel address used by the policy engine for native ETH.
pub const NATIVE_ETH_SENTINEL: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

/// Token metadata lookup used by Universal Router command decoders.
#[derive(Debug)]
pub struct TokenLookup {
    tokens: HashMap<(ChainId, String), Token>,
}

impl TokenLookup {
    /// Builds a lookup containing mainnet USDT, USDC, WETH, and native ETH.
    #[must_use]
    pub fn with_mainnet_defaults() -> Self {
        let mut me = Self {
            tokens: HashMap::new(),
        };
        me.add(Token {
            chain_id: 1,
            address: Address::from_alloy(address!("0xdac17f958d2ee523a2206206994597c13d831ec7")),
            symbol: "USDT".into(),
            decimals: 6,
            is_native: false,
        });
        me.add(Token {
            chain_id: 1,
            address: Address::from_alloy(address!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        });
        me.add(Token {
            chain_id: 1,
            address: Address::from_alloy(address!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")),
            symbol: "WETH".into(),
            decimals: 18,
            is_native: false,
        });
        me.add(native_eth(1));
        me
    }

    /// Adds or replaces one token by chain and address.
    pub fn add(&mut self, token: Token) {
        self.tokens.insert(
            (token.chain_id, token.address.as_str().to_lowercase()),
            token,
        );
    }

    /// Returns known metadata or an `UNKNOWN` token placeholder.
    #[must_use]
    pub fn get(&self, chain_id: ChainId, addr: &Address) -> Token {
        self.tokens
            .get(&(chain_id, addr.as_str().to_lowercase()))
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

/// All mainnet Universal Router contract addresses we recognize as a
/// "DEX swap" target. Uniswap has redeployed Universal Router several
/// times; if a user's wallet client hits a redeployment we don't list
/// here, the call gets classified as `other` instead of `dex` and
/// dex-targeted policies (e.g. max-input-usd caps) silently let it
/// through.
///
/// References:
/// - <https://docs.uniswap.org/contracts/v3/reference/deployments/ethereum-deployments>
/// - <https://github.com/Uniswap/universal-router/tree/main/deploy-addresses>
pub(crate) fn router_addresses_mainnet() -> Vec<Address> {
    vec![
        // Universal Router v1.2 (current canonical, hits most wallets)
        Address::from_alloy(address!("0x66a9893cc07d91d95644aedd05d03f95e1dba8af")),
        // Universal Router v2 (latest deployment Uniswap UI is routing
        // 1-of-N calls through as of 2026-05; observed in wild during
        // wallet_sendCalls swaps)
        Address::from_alloy(address!("0x4c82d1fbfe28c977cbb58d8c7ff8fcf9f70a2cca")),
        // Earlier deployments still active for some integrations
        Address::from_alloy(address!("0x3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad")),
        Address::from_alloy(address!("0xef1c6e67703c7bd7107eed8303fbe6ec2554bf6b")),
    ]
}

pub(crate) fn native_eth_address() -> Address {
    Address::from_alloy(address!("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"))
}

/// Construct a policy token for native ETH.
#[must_use]
pub fn native_eth(chain_id: ChainId) -> Token {
    Token {
        chain_id,
        address: native_eth_address(),
        symbol: "ETH".into(),
        decimals: 18,
        is_native: true,
    }
}

/// Convert Uniswap v4 zero currency into the policy native ETH sentinel.
#[must_use]
pub fn currency_to_policy_address(currency: alloy_primitives::Address) -> Address {
    if currency == alloy_primitives::Address::ZERO {
        native_eth_address()
    } else {
        Address::from_alloy(currency)
    }
}

/// Shift an integer decimal string right by `decimals` places.
#[must_use]
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
