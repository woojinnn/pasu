//! TokenHolding — the held balance state for a single fungibility unit.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::key::TokenKey;
use super::kind::TokenKind;
use crate::live_field::{DataSource, LiveField};
use crate::primitives::{Address, Price, Time, U256};

/// Representation of a held amount. ERC20/Native/ERC1155 carry a `Fungible`
/// quantity, while ERC721 only needs `Owned`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "form", rename_all = "snake_case")]
pub enum Balance {
    /// ERC20 / Native / ERC1155 — a quantity within the same fungibility unit.
    Fungible {
        /// Raw on-chain token amount (U256, serialized as a decimal string).
        #[tsify(type = "string")]
        amount: U256,
    },
    /// ERC721 — mere ownership is sufficient; the entry's existence means owned.
    Owned,
}

impl Balance {
    /// Builds a `Fungible` balance from any value convertible into `U256`.
    pub fn fungible(amount: impl Into<U256>) -> Self {
        Self::Fungible {
            amount: amount.into(),
        }
    }

    /// Builds a `Fungible` balance with a zero amount.
    pub fn zero_fungible() -> Self {
        Self::Fungible { amount: U256::ZERO }
    }

    /// Returns the fungible amount, or `None` for an `Owned` (ERC721) balance.
    pub fn as_fungible(&self) -> Option<U256> {
        match self {
            Self::Fungible { amount } => Some(*amount),
            Self::Owned => None,
        }
    }

    /// Returns `true` when the fungible amount is zero; always `false` for `Owned`.
    pub fn is_zero(&self) -> bool {
        match self {
            Self::Fungible { amount } => amount.is_zero(),
            // Owned entry 존재 자체가 보유. is_zero 의 의미 없음 → false.
            Self::Owned => false,
        }
    }
}

/// Held balance state of a single token (one fungibility unit) for an account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TokenHolding {
    /// Identity of the token (chain, standard, address, and optional token id).
    pub key: TokenKey,
    /// Token standard / classification (ERC20, Native, ERC721, ERC1155, ...).
    pub kind: TokenKind,
    /// Token symbol (e.g. "USDC", "WETH").
    pub symbol: String,
    /// Number of decimals; meaningless for ERC721 (conventionally 0).
    pub decimals: u8,

    /// Total held balance for this token.
    pub balance: Balance,
    /// Amount locked by pending changes; recomputed on sync per the §6.1 rules.
    pub committed: Balance,

    /// Per-token-id `approve(tokenId, spender)` for an individual ERC721 NFT;
    /// `None` for all other standards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub approved_to: Option<Address>,

    /// USD price, populated only for assets that can be priced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub price_usd: Option<LiveField<Price>>,

    /// Timestamp at which this holding's primitive fields were last synced.
    pub last_synced_at: Time,
    /// Provenance of the primitive (on-chain) fields in this holding.
    pub primitives_source: DataSource,
}

impl TokenHolding {
    /// Policy-view helper — the spendable balance, i.e. `balance` minus
    /// `committed`. Returns `None` for `Owned`, where the notion is meaningless.
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
