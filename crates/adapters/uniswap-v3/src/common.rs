//! Items shared across all Uniswap V3 SwapRouter function modules:
//! known router addresses, the token registry the function adapters consult
//! when they emit `Action`, and decimal helpers.

use policy_engine::prelude::*;
use std::collections::HashMap;

/// SwapRouter (the original Uniswap V3 router) on mainnet.
pub const SWAP_ROUTER_MAINNET: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";

/// Token registry baked into the adapter for v0.1. Production replaces this
/// with the manifest's `tokenLookup` capability.
///
/// Each per-function adapter (e.g., `exact_input_single`) holds one of these
/// to look up `Token` metadata for the addresses it decodes from calldata.
pub struct TokenLookup {
    tokens: HashMap<(ChainId, String), Token>,
}

impl TokenLookup {
    /// Builds a lookup pre-populated with USDT, USDC, and WETH on mainnet.
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

    /// Look up a token; returns a synthetic `UNKNOWN` placeholder when missing
    /// so adapters can still emit a structurally valid `Action`.
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

/// Shift `value` (decimal string of an integer) right by `decimals` places to
/// produce a human-readable decimal string.
///
/// Examples:
///   shift_decimals("200000000", 6)  == "200.000000"
///   shift_decimals("0", 6)          == "0.000000"
///   shift_decimals("1", 18)         == "0.000000000000000001"
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

/// Decode a Uniswap V3 packed `bytes path`.
///
/// Layout: `[tokenA (20)][fee0 (3)][tokenB (20)][fee1 (3)]...[tokenN (20)]`,
/// so `path.len() == 20 + 23 * hops`.
///
/// Returns `(tokens, fees)`: `tokens.len() == hops + 1`, `fees.len() == hops`.
pub fn decode_v3_path(
    path: &[u8],
) -> Result<(Vec<alloy_primitives::Address>, Vec<u32>), DecodeError> {
    if path.len() < 20 + 23 || !(path.len() - 20).is_multiple_of(23) {
        return Err(DecodeError::AbiDecode(format!(
            "invalid Uniswap V3 path length: {} (must be 20 + 23*N)",
            path.len()
        )));
    }
    let hops = (path.len() - 20) / 23;
    let mut tokens = Vec::with_capacity(hops + 1);
    let mut fees = Vec::with_capacity(hops);

    let mut cursor = 0;
    for _ in 0..hops {
        tokens.push(alloy_primitives::Address::from_slice(
            &path[cursor..cursor + 20],
        ));
        cursor += 20;
        let fee_bytes = &path[cursor..cursor + 3];
        let fee =
            ((fee_bytes[0] as u32) << 16) | ((fee_bytes[1] as u32) << 8) | (fee_bytes[2] as u32);
        fees.push(fee);
        cursor += 3;
    }
    tokens.push(alloy_primitives::Address::from_slice(
        &path[cursor..cursor + 20],
    ));
    Ok((tokens, fees))
}

/// Common decode error kinds used by the per-function modules.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("calldata too short: need at least {need} bytes, got {got}")]
    TooShort { need: usize, got: usize },
    #[error("unexpected selector: got 0x{got}, expected 0x{want}")]
    BadSelector { got: String, want: String },
    #[error("ABI decode failed: {0}")]
    AbiDecode(String),
    #[error("uint24 fee value {0} doesn't fit u32 (should never happen for valid V3 calldata)")]
    FeeOutOfRange(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_decimals_basic() {
        assert_eq!(shift_decimals("200000000", 6), "200.000000");
        assert_eq!(shift_decimals("1000000", 6), "1.000000");
        assert_eq!(shift_decimals("0", 6), "0.000000");
        assert_eq!(shift_decimals("1", 18), "0.000000000000000001");
    }

    #[test]
    fn shift_decimals_zero_decimals_passthrough() {
        assert_eq!(shift_decimals("12345", 0), "12345");
    }

    #[test]
    fn token_lookup_returns_known_tokens() {
        let lookup = TokenLookup::with_mainnet_defaults();
        let usdt = Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap();
        assert_eq!(lookup.get(1, &usdt).symbol, "USDT");
        assert_eq!(lookup.get(1, &usdt).decimals, 6);
    }

    #[test]
    fn token_lookup_falls_back_to_unknown() {
        let lookup = TokenLookup::with_mainnet_defaults();
        let unknown = Address::new("0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap();
        let t = lookup.get(1, &unknown);
        assert_eq!(t.symbol, "UNKNOWN");
        assert_eq!(t.decimals, 18);
    }

    #[test]
    fn token_lookup_keys_by_chain_id() {
        let lookup = TokenLookup::with_mainnet_defaults();
        let usdt = Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap();
        // USDT on Polygon is not registered → UNKNOWN fallback.
        assert_eq!(lookup.get(137, &usdt).symbol, "UNKNOWN");
    }

    #[test]
    fn v3_path_single_hop_decodes_two_tokens_one_fee() {
        // tokenA + fee + tokenB
        let mut path = Vec::new();
        path.extend_from_slice(&[0x11; 20]); // token A
        path.extend_from_slice(&[0x00, 0x0b, 0xb8]); // fee 3000
        path.extend_from_slice(&[0x22; 20]); // token B
        let (tokens, fees) = decode_v3_path(&path).unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(fees.len(), 1);
        assert_eq!(fees[0], 3000);
    }

    #[test]
    fn v3_path_multi_hop() {
        let mut path = Vec::new();
        path.extend_from_slice(&[0x11; 20]);
        path.extend_from_slice(&[0x00, 0x01, 0xf4]); // fee 500
        path.extend_from_slice(&[0x22; 20]);
        path.extend_from_slice(&[0x00, 0x0b, 0xb8]); // fee 3000
        path.extend_from_slice(&[0x33; 20]);
        let (tokens, fees) = decode_v3_path(&path).unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(fees, vec![500, 3000]);
    }

    #[test]
    fn v3_path_rejects_invalid_length() {
        let bad = vec![0u8; 22]; // not 20 + 23*N
        assert!(decode_v3_path(&bad).is_err());
        assert!(decode_v3_path(&[]).is_err());
    }
}
