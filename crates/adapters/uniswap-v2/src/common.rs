//! Shared resources for V2 Router02 swap-function adapters.

use alloy_primitives::{address, Address as AlloyAddress};
use policy_engine::prelude::*;
use std::collections::HashMap;

/// Uniswap V2 Router02 on mainnet.
pub const UNISWAP_V2_ROUTER_MAINNET: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

/// Sentinel address used to represent native ETH inside our `Token` model.
/// Not the same as any deployed token contract — it's purely an identifier.
pub const NATIVE_ETH_SENTINEL: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

/// Construct a `Token` representing native ETH on the given chain.
#[must_use]
pub fn native_eth(chain_id: ChainId) -> Token {
    Token {
        chain_id,
        address: Address::from_alloy(address!("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")),
        symbol: "ETH".into(),
        decimals: 18,
        is_native: true,
    }
}

pub(crate) fn router_address() -> Address {
    Address::from_alloy(address!("0x7a250d5630b4cf539739df2c5dacb4c659f2488d"))
}

#[allow(clippy::panic)]
pub(crate) fn static_adapter_id(raw: &str) -> AdapterId {
    match AdapterId::new(raw) {
        Ok(id) => id,
        Err(err) => panic!("invalid static adapter id {raw}: {err}"),
    }
}

pub(crate) fn path_endpoints(
    path: &[AlloyAddress],
) -> Result<(AlloyAddress, AlloyAddress), AdapterError> {
    if path.len() < 2 {
        return Err(AdapterError::BadCalldata(format!(
            "v2 path must contain at least 2 tokens, got {}",
            path.len()
        )));
    }
    Ok((path[0], path[path.len() - 1]))
}

/// Build a DEX swap action from decoded V2 router parameters.
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
) -> Action {
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

    Action::Dex(DexAction {
        actor: tx.from.clone(),
        target: tx.to.clone(),
        value_wei: tx.value_wei.clone(),
        facts: DexFacts {
            protocol_ids: vec![protocol_id.into()],
            input_tokens: vec![input_token],
            output_tokens: vec![output_token],
            total_input_usd: None,
            total_min_output_usd: None,
            max_fee_bps,
            has_zero_min_output,
            has_external_recipient: recipient != &tx.from,
            total_input_fraction_of_portfolio_bps: None,
            allowances_cover_inputs: None,
            window_stats: None,
        },
        oracle_requirements,
        trace: DexTrace {
            steps: vec![trace_step.into()],
        },
    })
}

/// Token metadata lookup used by V2 swap adapters.
#[derive(Debug)]
pub struct TokenLookup {
    tokens: HashMap<(ChainId, String), Token>,
}

impl TokenLookup {
    /// Builds a lookup containing mainnet USDT, USDC, and WETH.
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

/// Errors returned by V2 calldata decoders.
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
    /// Swap path did not contain both input and output tokens.
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

    #[test]
    fn dex_swap_action_sets_aggregate_dex_fields() {
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
            value_wei: "123".into(),
            data: Vec::new(),
            gas: None,
            nonce: None,
        };
        let input_token = Token {
            chain_id: 1,
            address: Address::new("0x0000000000000000000000000000000000000010").unwrap(),
            symbol: "IN".into(),
            decimals: 18,
            is_native: false,
        };
        let output_token = Token {
            chain_id: 1,
            address: Address::new("0x0000000000000000000000000000000000000020").unwrap(),
            symbol: "OUT".into(),
            decimals: 6,
            is_native: false,
        };

        match dex_swap_action(
            &tx,
            "uniswap-v2",
            input_token,
            output_token,
            "100".into(),
            Some("0".into()),
            &Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            Some(30),
            "uniswap-v2/test",
        ) {
            Action::Dex(d) => {
                assert_eq!(d.actor, tx.from);
                assert_eq!(d.target, tx.to);
                assert_eq!(d.value_wei, "123");
                assert_eq!(d.facts.protocol_ids, vec!["uniswap-v2".to_string()]);
                assert_eq!(d.facts.input_tokens[0].symbol, "IN");
                assert_eq!(d.facts.output_tokens[0].symbol, "OUT");
                assert_eq!(d.facts.max_fee_bps, Some(30));
                assert!(d.facts.has_zero_min_output);
                assert!(d.facts.has_external_recipient);
                assert_eq!(d.oracle_requirements.len(), 2);
                assert_eq!(d.oracle_requirements[0].kind, OracleRequirementKind::Input);
                assert_eq!(d.oracle_requirements[0].raw_amount, "100");
                assert_eq!(
                    d.oracle_requirements[1].kind,
                    OracleRequirementKind::MinOutput
                );
                assert_eq!(d.oracle_requirements[1].raw_amount, "0");
                assert_eq!(d.trace.steps, vec!["uniswap-v2/test".to_string()]);
            }
            other => panic!("expected dex, got {other:?}"),
        }
    }
}
