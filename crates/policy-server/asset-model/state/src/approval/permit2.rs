//! Permit2 block-level allowance state.
//!
//! Each entry records the amount, expiration, and nonce for a token-spender pair.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Time, U256};

/// Permit2 contract allowance recorded for a `(token, spender)` pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2Allowance {
    /// Allowance amount in the token's smallest unit.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Allowance expiration timestamp.
    pub expiration: Time,
    /// Permit2 spender-level nonce, incremented on re-signing.
    pub nonce: u32,
}
