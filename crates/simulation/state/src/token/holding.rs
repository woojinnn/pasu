//! TokenHolding — 한 fungibility 단위의 보유 상태.

use serde::{Deserialize, Serialize};

use super::key::TokenKey;
use super::kind::TokenKind;
use crate::live_field::{DataSource, LiveField};
use crate::primitives::{Address, Price, Time, U256};

/// 보유 양 표현. ERC20/Native/ERC1155 는 Fungible 수량, ERC721 은 Owned 만으로 충분.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "snake_case")]
pub enum Balance {
    /// ERC20 / Native / ERC1155 — 같은 fungibility 단위 안의 수량.
    Fungible { amount: U256 },
    /// ERC721 — 보유 사실 자체로 충분. entry 존재 = owned.
    Owned,
}

impl Balance {
    pub fn fungible(amount: impl Into<U256>) -> Self {
        Self::Fungible {
            amount: amount.into(),
        }
    }

    pub fn zero_fungible() -> Self {
        Self::Fungible { amount: U256::ZERO }
    }

    pub fn as_fungible(&self) -> Option<U256> {
        match self {
            Self::Fungible { amount } => Some(*amount),
            Self::Owned => None,
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Self::Fungible { amount } => amount.is_zero(),
            // Owned entry 존재 자체가 보유. is_zero 의 의미 없음 → false.
            Self::Owned => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenHolding {
    pub key: TokenKey,
    pub kind: TokenKind,
    pub symbol: String,
    /// ERC721 은 의미 없음 (관례적으로 0).
    pub decimals: u8,

    pub balance: Balance,
    /// pending 에 묶인 양. §6.1 규칙대로 sync 시 재계산.
    pub committed: Balance,

    /// ERC721 *개별 NFT* 에 대한 approve(tokenId, spender). 그 외 표준에선 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_to: Option<Address>,

    /// 가격 매김 가능한 자산만 채움.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_usd: Option<LiveField<Price>>,

    pub last_synced_at: Time,
    pub primitives_source: DataSource,
}

impl TokenHolding {
    /// 정책 view 헬퍼 — committed 를 뺀 사용 가능 잔액.
    /// Owned 인 경우 의미가 없으므로 None.
    pub fn available(&self) -> Option<U256> {
        match (&self.balance, &self.committed) {
            (
                Balance::Fungible { amount: bal },
                Balance::Fungible { amount: cmt },
            ) => Some(bal.saturating_sub(*cmt)),
            _ => None,
        }
    }
}
