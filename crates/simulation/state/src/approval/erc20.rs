//! ERC20 allowance — (owner, chain, token contract, spender) 의 spender 별 한도.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Time, U256};

/// ERC20 `approve` 로 spender 에게 부여된 한도.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AllowanceSpec {
    /// 한도 base unit (예: ETH 면 wei).
    #[tsify(type = "string")]
    pub amount: U256,
    /// 2^256-1 또는 `sufficiently_high` 한도. 정책의 빠른 검사용.
    pub is_unlimited: bool,
    /// 본 한도가 마지막으로 설정된 시각.
    pub last_set_at: Time,
}

impl AllowanceSpec {
    /// 정해진 한도와 시각으로 `AllowanceSpec` 생성. `is_unlimited` 는
    /// `amount == U256::MAX` 일 때 자동 true.
    pub fn new(amount: U256, last_set_at: Time) -> Self {
        Self {
            amount,
            is_unlimited: amount == U256::MAX,
            last_set_at,
        }
    }

    /// 무한 한도 (`U256::MAX`) `AllowanceSpec` 생성.
    pub fn unlimited(last_set_at: Time) -> Self {
        Self {
            amount: U256::MAX,
            is_unlimited: true,
            last_set_at,
        }
    }
}
