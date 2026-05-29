//! Permit2 block-level allowance for a (token, spender) pair: amount, expiration, and nonce.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Time, U256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// Permit2 allowance granted to a spender: approved amount, its expiration time, and the current nonce.
pub struct Permit2Allowance {
    /// Approved spending amount (256-bit unsigned).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Unix timestamp (seconds) at which this allowance expires.
    pub expiration: Time,
    /// Nonce incremented on each allowance update, used to invalidate prior approvals.
    pub nonce: u32,
}
