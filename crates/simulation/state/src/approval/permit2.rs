//! Permit2 — block-level allowance (token, spender) 의 expiration / nonce.

use serde::{Deserialize, Serialize};

use crate::primitives::{Time, U256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permit2Allowance {
    pub amount: U256,
    pub expiration: Time,
    pub nonce: u32,
}
