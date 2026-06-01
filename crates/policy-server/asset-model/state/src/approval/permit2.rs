//! Permit2 — block-level allowance (token, spender) 의 expiration / nonce.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Time, U256};

/// Permit2 contract 에 기록된 (token, spender) 권한 — 한도 / 만료 / nonce.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2Allowance {
    /// 한도 base unit (token 의 smallest unit).
    #[tsify(type = "string")]
    pub amount: U256,
    /// 한도 만료 시각.
    pub expiration: Time,
    /// Permit2 의 spender-level nonce. 재서명 시 1 증가.
    pub nonce: u32,
}
