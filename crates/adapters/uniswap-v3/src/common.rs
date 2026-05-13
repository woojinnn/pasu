//! Items shared across all Uniswap V3 `SwapRouter` function modules:
//! known router addresses, the token registry the function adapters consult
//! when they emit `Action`, and decimal helpers.

use alloy_primitives::{address, Address as AlloyAddress};
use policy_engine::prelude::*;
use std::collections::{HashMap, HashSet};

/// `SwapRouter` (the original Uniswap V3 router) on mainnet.
pub const SWAP_ROUTER_MAINNET: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";

/// Token registry baked into the adapter for v0.1. Production replaces this
/// with the manifest's `tokenLookup` capability.
///
/// Each per-function adapter (e.g., `exact_input_single`) holds one of these
/// to look up `Token` metadata for the addresses it decodes from calldata.
#[derive(Debug)]
pub struct TokenLookup {
    tokens: HashMap<(ChainId, String), Token>,
}

impl TokenLookup {
    /// Builds a lookup pre-populated with USDT, USDC, and WETH on mainnet.
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
        me
    }

    /// Adds or replaces one token by chain and address.
    pub fn add(&mut self, token: Token) {
        self.tokens.insert(
            (token.chain_id, token.address.as_str().to_lowercase()),
            token,
        );
    }

    /// Returns this lookup after adding `token`.
    #[must_use]
    pub fn with(mut self, token: Token) -> Self {
        self.add(token);
        self
    }

    /// Look up a token; returns a synthetic `UNKNOWN` placeholder when missing
    /// so adapters can still emit a structurally valid `LegacyAction`.
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

pub(crate) fn swap_router_address() -> Address {
    Address::from_alloy(address!("0xe592427a0aece92de3edee1f18e0157c05861564"))
}

#[allow(clippy::panic)]
pub(crate) fn static_adapter_id(raw: &str) -> ActionAdapterId {
    match ActionAdapterId::new(raw) {
        Ok(id) => id,
        Err(err) => panic!("invalid static adapter id {raw}: {err}"),
    }
}

pub(crate) fn path_endpoints(
    path: &[AlloyAddress],
) -> Result<(AlloyAddress, AlloyAddress), ActionAdapterError> {
    if path.len() < 2 {
        return Err(ActionAdapterError::BadCalldata(format!(
            "v3 path must contain at least 2 tokens, got {}",
            path.len()
        )));
    }
    Ok((path[0], path[path.len() - 1]))
}

/// Build the aggregate DEX action emitted by a single decoded swap call.
#[allow(clippy::too_many_arguments)]
pub fn dex_swap_action(
    tx: &TransactionRequest,
    protocol_id: &str,
    input_token: Token,
    output_token: Token,
    input_raw: String,
    min_output_raw: Option<String>,
    recipient: &Address,
    max_fee_bps: Option<u32>,
    trace_step: impl Into<String>,
) -> LegacyAction {
    let mut oracle_requirements = vec![OracleRequirement {
        kind: OracleRequirementKind::Input,
        token: input_token.clone(),
        raw_amount: input_raw,
    }];

    let has_zero_min_output = min_output_raw.as_deref() == Some("0");
    if let Some(raw_amount) = min_output_raw {
        oracle_requirements.push(OracleRequirement {
            kind: OracleRequirementKind::MinOutput,
            token: output_token.clone(),
            raw_amount,
        });
    }

    LegacyAction::Dex(DexAction {
        actor: tx.from.clone(),
        target: tx.to.clone(),
        value_wei: tx.value_wei.clone(),
        facts: DexFacts {
            protocol_ids: vec![protocol_id.into()],
            input_tokens: vec![input_token],
            output_tokens: vec![output_token],
            max_fee_bps,
            has_zero_min_output,
            has_external_recipient: recipient != &tx.from,
            ..DexFacts::default()
        },
        oracle_requirements,
        trace: DexTrace {
            steps: vec![trace_step.into()],
        },
    })
}

/// Merge child DEX actions from a structural router call into one aggregate.
///
/// # Errors
///
/// Returns an error if any child action is not a DEX action.
pub fn merge_dex_actions(
    tx: &TransactionRequest,
    actions: Vec<LegacyAction>,
    trace_step: impl Into<String>,
) -> Result<LegacyAction, ActionAdapterError> {
    let mut protocol_ids = Vec::new();
    let mut seen_protocol_ids = HashSet::new();
    let mut input_tokens = Vec::new();
    let mut seen_input_tokens = HashSet::new();
    let mut output_tokens = Vec::new();
    let mut seen_output_tokens = HashSet::new();
    let mut oracle_requirements = Vec::new();
    let mut trace_steps = vec![trace_step.into()];
    let mut max_fee_bps: Option<u32> = None;
    let mut has_zero_min_output = false;
    let mut has_external_recipient = false;

    for action in actions {
        let LegacyAction::Dex(dex) = action else {
            return Err(ActionAdapterError::BadCalldata(format!(
                "multicall child emitted non-dex action: {}",
                action.kind()
            )));
        };

        for protocol_id in dex.facts.protocol_ids {
            if seen_protocol_ids.insert(protocol_id.clone()) {
                protocol_ids.push(protocol_id);
            }
        }
        for token in dex.facts.input_tokens {
            if seen_input_tokens.insert(token.key()) {
                input_tokens.push(token);
            }
        }
        for token in dex.facts.output_tokens {
            if seen_output_tokens.insert(token.key()) {
                output_tokens.push(token);
            }
        }
        if let Some(fee) = dex.facts.max_fee_bps {
            max_fee_bps = Some(max_fee_bps.map_or(fee, |current| current.max(fee)));
        }
        has_zero_min_output |= dex.facts.has_zero_min_output;
        has_external_recipient |= dex.facts.has_external_recipient;
        oracle_requirements.extend(dex.oracle_requirements);
        trace_steps.extend(dex.trace.steps);
    }

    Ok(LegacyAction::Dex(DexAction {
        actor: tx.from.clone(),
        target: tx.to.clone(),
        value_wei: tx.value_wei.clone(),
        facts: DexFacts {
            protocol_ids,
            input_tokens,
            output_tokens,
            max_fee_bps,
            has_zero_min_output,
            has_external_recipient,
            ..DexFacts::default()
        },
        oracle_requirements,
        trace: DexTrace { steps: trace_steps },
    }))
}

/// Shift `value` (decimal string of an integer) right by `decimals` places to
/// produce a human-readable decimal string.
///
/// Examples:
///   `shift_decimals("200000000`", 6)  == "200.000000"
///   `shift_decimals("0`", 6)          == "0.000000"
///   `shift_decimals("1`", 18)         == "0.000000000000000001"
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

/// Decode a Uniswap V3 packed `bytes path`.
///
/// Decode a Uniswap V3 packed path.
///
/// Thin wrapper that delegates to
/// [`abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path`] and
/// remaps its error into this adapter's local [`DecodeError`].
///
/// # Errors
///
/// Returns an error if the path length is not a valid V3 packed path length.
pub fn decode_v3_path(path: &[u8]) -> Result<(Vec<AlloyAddress>, Vec<u32>), DecodeError> {
    abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path(path)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))
}

/// Common decode error kinds used by the per-function modules.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    /// Calldata is shorter than the minimum required length.
    #[error("calldata too short: need at least {need} bytes, got {got}")]
    TooShort {
        /// Required byte length.
        need: usize,
        /// Actual byte length.
        got: usize,
    },
    /// The four-byte selector does not match the expected function.
    #[error("unexpected selector: got 0x{got}, expected 0x{want}")]
    BadSelector {
        /// Observed selector hex.
        got: String,
        /// Expected selector hex.
        want: String,
    },
    /// ABI decoding failed.
    #[error("ABI decode failed: {0}")]
    AbiDecode(String),
    /// A decoded uint24 fee could not be widened.
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
