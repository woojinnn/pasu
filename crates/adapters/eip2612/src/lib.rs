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

use policy_engine::adapter::signature_helpers::{
    address_field, object, static_adapter_id, static_token, u256_string_field, u64_field,
    TokenLookup,
};
use policy_engine::prelude::*;

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
            tokens: default_token_lookup(),
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
        if !sig.primary_type().eq_ignore_ascii_case("Permit") {
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

fn default_token_lookup() -> TokenLookup {
    TokenLookup::with_tokens([
        static_token(1, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
        static_token(137, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
        static_token(1, "0xdac17f958d2ee523a2206206994597c13d831ec7", "USDT", 6),
        static_token(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18),
    ])
}
