//! AssetCommitment — 한 pending 이 자산을 어떻게 묶고 있는지.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, U256};
use crate::token::TokenRef;

/// pending 이 자산에 미치는 영향. spec §6 committed 갱신 규칙의 입력.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetCommitment {
    /// 한도형 — venue/spender 가 최대 `max_out` 까지 가져갈 수 있음 (`UniswapX`, permit).
    SpendCap {
        /// 묶인 토큰.
        token: TokenRef,
        /// 최대 spend 가능량 (base unit).
        #[tsify(type = "string")]
        max_out: U256,
    },

    /// 확정형 — 이미 venue/계약이 들고 있음 (perp margin lock). 잔고 자체에 이미 반영됨.
    HardLock {
        /// 묶인 토큰.
        token: TokenRef,
        /// venue 가 들고 있는 양 (base unit).
        #[tsify(type = "string")]
        locked: U256,
    },

    /// Permit 발급만 — token 은 잠긴 게 아니지만 spend 권한이 부여됨.
    PermitCap {
        /// 권한이 부여된 토큰.
        token: TokenRef,
        /// 권한을 받은 spender 주소.
        #[tsify(type = "string")]
        spender: Address,
        /// 최대 spend 가능량 (base unit).
        #[tsify(type = "string")]
        max_out: U256,
    },

    /// 없음 (reduce-only, 수령형 주문).
    None,
}

impl AssetCommitment {
    /// committed 합산에 들어갈지 판정. spec §6.1.
    /// - `SpendCap` / `PermitCap` → committed 에 합산
    /// - `HardLock` → 잔고에 이미 반영, 합산 안 함
    /// - None → 0
    pub fn cap_for(&self, key: &crate::token::TokenKey) -> U256 {
        match self {
            Self::SpendCap { token, max_out } if &token.key == key => *max_out,
            Self::PermitCap { token, max_out, .. } if &token.key == key => *max_out,
            _ => U256::ZERO,
        }
    }
}
