//! TokenKey — fungibility 단위 식별자.
//!
//! 같은 ERC20 contract 안의 모든 unit 은 fungible 이므로 (chain, address) 만으로
//! key 가 결정된다. ERC721 / ERC1155 는 같은 contract 라도 token_id 가 다르면
//! 별개 자산이므로 token_id 까지 포함한다.

use serde::{Deserialize, Serialize};

use crate::primitives::{Address, ChainId, U256};

/// ERC721/1155 의 token id. uint256 까지 표현 가능.
pub type TokenId = U256;

/// 한 holding 의 fungibility 단위. 같은 key 안의 모든 unit 은 서로 교환 가능.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "standard", rename_all = "snake_case")]
pub enum TokenKey {
    /// 체인의 native gas 자산 (ETH on Ethereum, SOL on Solana 등).
    Native { chain: ChainId },

    /// ERC20 — contract 단위가 곧 fungibility 단위.
    Erc20 { chain: ChainId, address: Address },

    /// ERC721 — (contract, token_id) 쌍이 고유.
    /// Uniswap V3/V4 LP NFT, Sudoswap pool LP 등.
    Erc721 {
        chain: ChainId,
        contract: Address,
        token_id: TokenId,
    },

    /// ERC1155 — 같은 token_id 끼리는 fungible, 다른 id 끼리는 별개.
    /// 게임 아이템, Trader Joe LB bin token 등.
    Erc1155 {
        chain: ChainId,
        contract: Address,
        token_id: TokenId,
    },
}

impl TokenKey {
    pub fn chain(&self) -> &ChainId {
        match self {
            Self::Native { chain }
            | Self::Erc20 { chain, .. }
            | Self::Erc721 { chain, .. }
            | Self::Erc1155 { chain, .. } => chain,
        }
    }

    /// ERC20/721/1155 일 때 contract 주소를 반환. Native 면 None.
    pub fn contract(&self) -> Option<&Address> {
        match self {
            Self::Native { .. } => None,
            Self::Erc20 { address, .. } => Some(address),
            Self::Erc721 { contract, .. } | Self::Erc1155 { contract, .. } => Some(contract),
        }
    }

    pub fn token_id(&self) -> Option<&TokenId> {
        match self {
            Self::Erc721 { token_id, .. } | Self::Erc1155 { token_id, .. } => Some(token_id),
            _ => None,
        }
    }

    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }

    pub fn is_nft(&self) -> bool {
        matches!(self, Self::Erc721 { .. })
    }
}
