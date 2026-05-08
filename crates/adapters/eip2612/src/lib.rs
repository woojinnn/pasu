//! EIP-2612 Permit EIP-712 signature adapter.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

use alloy_primitives::U256;
use policy_engine::prelude::*;
use serde_json::{Map, Value};
use std::collections::HashMap;

const UINT256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

/// EIP-2612 Permit adapter.
#[derive(Debug, Clone)]
pub struct Eip2612Adapter {
    tokens: TokenLookup,
}

impl Eip2612Adapter {
    /// Construct an adapter with common mainnet token metadata.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tokens: TokenLookup::with_defaults(),
        }
    }

    /// Returns this adapter after adding `token` as a routed verifying
    /// contract.
    #[must_use]
    pub fn with_token(mut self, token: Token) -> Self {
        self.tokens.add(token);
        self
    }
}

impl Default for Eip2612Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureAdapter for Eip2612Adapter {
    fn id(&self) -> AdapterId {
        static_adapter_id("eip2612/permit@0.1.0")
    }

    fn match_keys(&self) -> Vec<SignatureMatchKey> {
        self.tokens
            .targets()
            .into_iter()
            .map(|(chain_id, verifying_contract)| {
                SignatureMatchKey::exact(chain_id, verifying_contract, "Permit")
            })
            .collect()
    }

    fn build(&self, sig: &SignatureRequest) -> Result<Action, AdapterError> {
        if sig.primary_type() != "Permit" {
            return Err(AdapterError::BadCalldata(format!(
                "unsupported EIP-2612 primaryType {}",
                sig.primary_type()
            )));
        }

        let message = object(&sig.typed_data.message, "message")?;
        let owner = address_field(message, "owner").map_err(|err| match err {
            AdapterError::BadCalldata(reason) => {
                AdapterError::BadCalldata(format!("invalid message.owner: {reason}"))
            }
        })?;
        let spender = address_field(message, "spender")?;
        let value = u256_string_field(message, "value")?;
        let deadline = u64_field(message, "deadline")?;
        let nonce = u256_string_field(message, "nonce")?;
        let token = self
            .tokens
            .get(sig.chain_id, &sig.typed_data.domain.verifying_contract);

        Ok(Action::Eip2612(Eip2612Action {
            signer: sig.signer.clone(),
            owner,
            chain_id: sig.chain_id,
            domain_chain_id: sig.typed_data.domain.chain_id,
            verifying_contract: sig.typed_data.domain.verifying_contract.clone(),
            primary_type: sig.typed_data.primary_type.clone(),
            spender,
            token,
            is_unlimited: value == UINT256_MAX_DEC,
            nonce_valid: nonce != UINT256_MAX_DEC,
            value,
            deadline,
            nonce,
            total_approved_usd: None,
        }))
    }
}

#[derive(Debug, Clone)]
struct TokenLookup {
    tokens: HashMap<(ChainId, String), Token>,
}

impl TokenLookup {
    fn with_defaults() -> Self {
        let mut lookup = Self {
            tokens: HashMap::new(),
        };
        lookup.add(token(
            1,
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "USDC",
            6,
        ));
        lookup.add(token(
            137,
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "USDC",
            6,
        ));
        lookup.add(token(
            1,
            "0xdac17f958d2ee523a2206206994597c13d831ec7",
            "USDT",
            6,
        ));
        lookup.add(token(
            1,
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "WETH",
            18,
        ));
        lookup
    }

    fn add(&mut self, token: Token) {
        self.tokens.insert(
            (token.chain_id, token.address.as_str().to_lowercase()),
            token,
        );
    }

    fn get(&self, chain_id: ChainId, address: &Address) -> Token {
        self.tokens
            .get(&(chain_id, address.as_str().to_lowercase()))
            .cloned()
            .unwrap_or_else(|| Token {
                chain_id,
                address: address.clone(),
                symbol: "UNKNOWN".into(),
                decimals: 18,
                is_native: false,
            })
    }

    fn targets(&self) -> Vec<(ChainId, Address)> {
        self.tokens
            .values()
            .map(|token| (token.chain_id, token.address.clone()))
            .collect()
    }
}

fn token(chain_id: ChainId, address: &str, symbol: &str, decimals: u32) -> Token {
    Token {
        chain_id,
        address: Address::new(address).unwrap_or_else(|err| {
            panic_static(&format!("invalid static token address {address}: {err}"))
        }),
        symbol: symbol.into(),
        decimals,
        is_native: false,
    }
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, AdapterError> {
    value
        .as_object()
        .ok_or_else(|| AdapterError::BadCalldata(format!("{label} must be an object")))
}

fn address_field(object: &Map<String, Value>, field: &str) -> Result<Address, AdapterError> {
    let value = stringish_field(object, field)?;
    Address::new(&value).map_err(AdapterError::BadCalldata)
}

fn u64_field(object: &Map<String, Value>, field: &str) -> Result<u64, AdapterError> {
    let value = u256_string_field(object, field)?;
    value
        .parse::<u64>()
        .map_err(|err| AdapterError::BadCalldata(format!("{field} does not fit u64: {err}")))
}

fn u256_string_field(object: &Map<String, Value>, field: &str) -> Result<String, AdapterError> {
    let value = stringish_field(object, field)?;
    U256::from_str_radix(&value, 10)
        .map(|parsed| parsed.to_string())
        .map_err(|err| AdapterError::BadCalldata(format!("{field} must be uint256: {err}")))
}

fn stringish_field(object: &Map<String, Value>, field: &str) -> Result<String, AdapterError> {
    let value = object
        .get(field)
        .ok_or_else(|| AdapterError::BadCalldata(format!("missing field {field}")))?;
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(AdapterError::BadCalldata(format!(
            "{field} must be a string or number"
        ))),
    }
}

#[allow(clippy::panic)]
fn static_adapter_id(raw: &str) -> AdapterId {
    AdapterId::new(raw).unwrap_or_else(|err| panic!("invalid static adapter id {raw}: {err}"))
}

#[allow(clippy::panic)]
fn panic_static(message: &str) -> ! {
    panic!("{message}");
}
