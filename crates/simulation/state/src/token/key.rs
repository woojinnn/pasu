//! `TokenKey` — identifier for a fungibility unit.
//!
//! All units within the same ERC20 contract are fungible, so the key is
//! determined by `(chain, address)` alone. For ERC721 / ERC1155, tokens in the
//! same contract with different `token_id`s are distinct assets, so the key
//! also includes the `token_id`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, ChainId, U256};

/// Token id for ERC721/1155 assets; representable up to a uint256.
pub type TokenId = U256;

/// Fungibility unit of a holding. All units sharing the same key are mutually interchangeable.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "standard", rename_all = "snake_case")]
pub enum TokenKey {
    /// The chain's native gas asset (e.g. ETH on Ethereum, SOL on Solana).
    Native {
        /// Chain on which the native asset lives.
        chain: ChainId,
    },

    /// ERC20 — the contract itself is the fungibility unit.
    Erc20 {
        /// Chain hosting the ERC20 contract.
        chain: ChainId,
        /// ERC20 contract address.
        #[tsify(type = "string")]
        address: Address,
    },

    /// ERC721 — the (contract, `token_id`) pair is unique.
    /// E.g. Uniswap V3/V4 LP NFTs, Sudoswap pool LP.
    Erc721 {
        /// Chain hosting the ERC721 contract.
        chain: ChainId,
        /// ERC721 contract address.
        #[tsify(type = "string")]
        contract: Address,
        /// Unique token id within the contract.
        #[tsify(type = "string")]
        token_id: TokenId,
    },

    /// ERC1155 — units with the same `token_id` are fungible, different ids are distinct.
    /// E.g. game items, Trader Joe LB bin tokens.
    Erc1155 {
        /// Chain hosting the ERC1155 contract.
        chain: ChainId,
        /// ERC1155 contract address.
        #[tsify(type = "string")]
        contract: Address,
        /// Token id whose units are fungible among themselves.
        #[tsify(type = "string")]
        token_id: TokenId,
    },
}

impl TokenKey {
    /// Returns the chain this token key belongs to.
    #[must_use]
    pub const fn chain(&self) -> &ChainId {
        match self {
            Self::Native { chain }
            | Self::Erc20 { chain, .. }
            | Self::Erc721 { chain, .. }
            | Self::Erc1155 { chain, .. } => chain,
        }
    }

    /// Returns the contract address for ERC20/721/1155; `None` for `Native`.
    #[must_use]
    pub const fn contract(&self) -> Option<&Address> {
        match self {
            Self::Native { .. } => None,
            Self::Erc20 { address, .. } => Some(address),
            Self::Erc721 { contract, .. } | Self::Erc1155 { contract, .. } => Some(contract),
        }
    }

    /// Returns the token id for ERC721/1155; `None` for `Native`/`Erc20`.
    #[must_use]
    pub const fn token_id(&self) -> Option<&TokenId> {
        match self {
            Self::Erc721 { token_id, .. } | Self::Erc1155 { token_id, .. } => Some(token_id),
            _ => None,
        }
    }

    /// Returns `true` if this key denotes the chain's native asset.
    #[must_use]
    pub const fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }

    /// Returns `true` if this key denotes an ERC721 NFT.
    #[must_use]
    pub const fn is_nft(&self) -> bool {
        matches!(self, Self::Erc721 { .. })
    }
}
