//! `MarketItem` — one offer or consideration leg of a marketplace order.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use crate::Bytes;

/// Asset class of a [`MarketItem`] (Seaport `ItemType`). `*Criteria` kinds bind
/// a Merkle root / whole collection (any token), NOT a concrete tokenId.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum MarketItemKind {
    /// Native coin (ETH). No `token`.
    Native,
    /// ERC-20 fungible token.
    Erc20,
    /// Concrete ERC-721 NFT (specific tokenId).
    Erc721,
    /// Concrete ERC-1155 semi-fungible (specific id).
    Erc1155,
    /// ERC-721 bound by criteria (Merkle root / `0` = any token in collection).
    Erc721Criteria,
    /// ERC-1155 bound by criteria.
    Erc1155Criteria,
}

/// One leg of a marketplace order: an offered item (what the maker gives /
/// taker receives) or a consideration item (what is paid, to a `recipient`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MarketItem {
    /// Position of this leg within its offer/consideration array. Preserves
    /// order + distinctness: the Cedar projection is a `Set`, which would
    /// otherwise dedup identical legs (e.g. two equal royalty splits).
    pub idx: u32,
    /// Asset class of this leg.
    pub kind: MarketItemKind,
    /// Token / NFT contract address. Absent for `native`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub token: Option<Address>,
    /// Concrete tokenId — present only for `erc721` / `erc1155` (NOT criteria).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub token_id: Option<U256>,
    /// Criteria Merkle root (bytes32 hex) — present only for `*_criteria` kinds.
    /// `0x0…0` means "any token in collection". NOT a tokenId.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub criteria_root: Option<Bytes>,
    /// Start amount (U256). `!= end_amount` ⇒ Dutch (time-interpolated) auction.
    #[tsify(type = "string")]
    pub start_amount: U256,
    /// End amount (U256).
    #[tsify(type = "string")]
    pub end_amount: U256,
    /// Recipient paid this leg — present only on consideration items (offerer
    /// proceeds / marketplace fee / creator royalty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub recipient: Option<Address>,
}
