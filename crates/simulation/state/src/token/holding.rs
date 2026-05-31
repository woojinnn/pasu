//! `TokenHolding` — the held balance state for a single fungibility unit.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::key::TokenKey;
use super::kind::TokenKind;
use crate::live_field::{DataSource, LiveField};
use crate::primitives::{Address, Price, Time, U256};

/// 보유 양 표현. ERC20/Native/ERC1155 는 Fungible 수량, ERC721 은 Owned 만으로 충분.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "form", rename_all = "snake_case")]
pub enum Balance {
    /// ERC20 / Native / ERC1155 — 같은 fungibility 단위 안의 수량.
    Fungible {
        /// base unit 수량 (예: ETH 면 wei).
        #[tsify(type = "string")]
        amount: U256,
    },
    /// ERC721 — 보유 사실 자체로 충분. entry 존재 = owned.
    Owned,
}

impl Balance {
    /// `U256` 수량으로 `Balance::Fungible` 생성.
    pub fn fungible(amount: impl Into<U256>) -> Self {
        Self::Fungible {
            amount: amount.into(),
        }
    }

    /// Builds a `Fungible` balance with a zero amount.
    #[must_use]
    pub const fn zero_fungible() -> Self {
        Self::Fungible { amount: U256::ZERO }
    }

    /// Returns the fungible amount, or `None` for an `Owned` (ERC721) balance.
    #[must_use]
    pub const fn as_fungible(&self) -> Option<U256> {
        match self {
            Self::Fungible { amount } => Some(*amount),
            Self::Owned => None,
        }
    }

    /// Returns `true` when the fungible amount is zero; always `false` for `Owned`.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        match self {
            Self::Fungible { amount } => amount.is_zero(),
            // Owned entry 존재 자체가 보유. is_zero 의 의미 없음 → false.
            Self::Owned => false,
        }
    }
}

/// 한 fungibility 단위의 보유 상태 + 메타 (kind, symbol, decimals, approval, price).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TokenHolding {
    /// 본 holding 의 fungibility 단위 key.
    pub key: TokenKey,
    /// 토큰의 의미 분류 (`TokenKind`).
    pub kind: TokenKind,
    /// 토큰 심볼 (예: "USDC").
    pub symbol: String,
    /// ERC721 은 의미 없음 (관례적으로 0).
    pub decimals: u8,

    /// 현재 보유 양.
    pub balance: Balance,
    /// pending 에 묶인 양. §6.1 규칙대로 sync 시 재계산.
    pub committed: Balance,

    /// ERC721 *개별 NFT* 에 대한 approve(tokenId, spender). 그 외 표준에선 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub approved_to: Option<Address>,

    /// 가격 매김 가능한 자산만 채움.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub price_usd: Option<LiveField<Price>>,

    /// 본 holding 이 마지막으로 sync 된 시각.
    pub last_synced_at: Time,
    /// `key` / `balance` / `kind` 등 primitive 필드의 출처.
    pub primitives_source: DataSource,
}

impl TokenHolding {
    /// Policy-view helper — the spendable balance, i.e. `balance` minus
    /// `committed`. Returns `None` for `Owned`, where the notion is meaningless.
    #[must_use]
    pub const fn available(&self) -> Option<U256> {
        match (&self.balance, &self.committed) {
            (Balance::Fungible { amount: bal }, Balance::Fungible { amount: cmt }) => {
                Some(bal.saturating_sub(*cmt))
            }
            _ => None,
        }
    }
}
