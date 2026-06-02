//! ERC20 per-spender allowance state.
//!
//! The allowance is keyed by owner, chain, token contract, and spender.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Time, U256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// An ERC20 approval limit granted to a single spender.
pub struct AllowanceSpec {
    /// Raw approved allowance amount (U256, serialized as a string).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Whether the allowance is effectively unlimited (2^256-1 or a sufficiently high cap); used for fast policy checks.
    pub is_unlimited: bool,
    /// Timestamp of the most recent `approve` that set this allowance.
    pub last_set_at: Time,
}

impl AllowanceSpec {
    /// Creates an allowance for the given amount, marking it unlimited when the amount equals `U256::MAX`.
    #[must_use]
    pub fn new(amount: U256, last_set_at: Time) -> Self {
        Self {
            amount,
            is_unlimited: amount == U256::MAX,
            last_set_at,
        }
    }

    /// Creates an explicitly unlimited allowance set to `U256::MAX`.
    #[must_use]
    pub const fn unlimited(last_set_at: Time) -> Self {
        Self {
            amount: U256::MAX,
            is_unlimited: true,
            last_set_at,
        }
    }
}
