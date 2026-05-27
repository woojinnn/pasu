//! ERC20 allowance — (owner, chain, token contract, spender) 의 spender 별 한도.

use serde::{Deserialize, Serialize};

use crate::primitives::{Time, U256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowanceSpec {
    pub amount: U256,
    /// 2^256-1 또는 sufficiently_high 한도. 정책의 빠른 검사용.
    pub is_unlimited: bool,
    pub last_set_at: Time,
}

impl AllowanceSpec {
    pub fn new(amount: U256, last_set_at: Time) -> Self {
        Self {
            amount,
            is_unlimited: amount == U256::MAX,
            last_set_at,
        }
    }

    pub fn unlimited(last_set_at: Time) -> Self {
        Self {
            amount: U256::MAX,
            is_unlimited: true,
            last_set_at,
        }
    }
}
