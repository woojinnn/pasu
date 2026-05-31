//! `TokenHolding` — the held balance state for a single fungibility unit.

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

/// Off-chain registry metadata for a token (logo, description, etc.).
/// Sourced from `CoinGecko` (`/coins/{platform}/contract/{address}`) on
/// first wallet sync; cached in the `tokens` table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TokenMetadata {
    /// CDN URL of the token icon (large/standard variant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub logo_url: Option<String>,
    /// Project homepage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub website_url: Option<String>,
    /// Short marketing description (may include markdown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub description: Option<String>,
    /// `CoinGecko` id (e.g. "usd-coin") so the UI can deep-link if needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub coingecko_id: Option<String>,
}

impl TokenMetadata {
    /// True when every field is `None` — used to skip serialization noise.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.logo_url.is_none()
            && self.website_url.is_none()
            && self.description.is_none()
            && self.coingecko_id.is_none()
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

    /// Off-chain registry metadata (logo, website, description). Sourced
    /// from `CoinGecko` on first wallet sync; `None` for tokens whose
    /// contract address isn't in `CoinGecko`'s catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub metadata: Option<TokenMetadata>,

    /// Computed USD value (`balance / 10^decimals × price_usd.value`).
    /// Set by server read handlers before serialization; never
    /// persisted. Use [`Self::compute_value_usd`] to recompute on the
    /// fly if you have just the primitives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub value_usd: Option<Price>,

    /// Timestamp at which this holding's primitive fields were last synced.
    pub last_synced_at: Time,
    /// Provenance of the primitive (on-chain) fields in this holding.
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

    /// Compute `balance / 10^decimals × price_usd.value` as a Decimal
    /// string. Returns `None` for non-fungible balances or when price
    /// data is missing. f64-based — fine for display, not for accounting.
    #[must_use]
    pub fn compute_value_usd(&self) -> Option<Price> {
        let bal = self.balance.as_fungible()?;
        let price = self.price_usd.as_ref()?;
        let price_str = price.value.as_str();
        let price_f: f64 = price_str.parse().ok()?;
        // U256 → f64 via decimal string; safe for display-scale values.
        let bal_str = bal.to_string();
        let bal_f: f64 = bal_str.parse().ok()?;
        let divisor = 10f64.powi(i32::from(self.decimals));
        if divisor <= 0.0 {
            return None;
        }
        let value = bal_f / divisor * price_f;
        // Avoid scientific notation for tiny dust amounts.
        Some(Price::new(format!("{value:.6}")))
    }
}
