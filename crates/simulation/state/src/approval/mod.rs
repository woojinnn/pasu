//! ApprovalSet — scope 별 분리된 wallet-level 권한 컬렉션. spec §4.4.
//!
//! ERC721 *per-token* approve(tokenId, spender) 만 TokenHolding.approved_to 에
//! nested (그 holding 과 1:1 이라 자연스러움). 나머지는 모두 여기 평탄 컬렉션.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod erc20;
pub mod permit2;

pub use erc20::AllowanceSpec;
pub use permit2::Permit2Allowance;

use crate::primitives::{Address, ChainId, Spender};

/// 한 컨트랙트를 (chain, contract address) 로 식별.
pub type ContractAddrKey = (ChainId, Address);

/// 한 (chain, contract, spender) 트리플로 식별.
pub type SpenderKey = (ChainId, Address, Spender);

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSet {
    /// ERC20 allowance.
    /// (chain, token contract) → spender 별 한도.
    /// (tuple key 라 JSON pairs 로 직렬화.)
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    pub erc20: BTreeMap<ContractAddrKey, BTreeMap<Spender, AllowanceSpec>>,

    /// ERC721/ERC1155 setApprovalForAll.
    /// (chain, NFT/1155 contract) → set-for-all 권한이 부여된 spender 들.
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    pub set_for_all: BTreeMap<ContractAddrKey, BTreeSet<Spender>>,

    /// Permit2 contract 기록상의 allowance.
    /// (chain, token contract, spender) → 한도.
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    pub permit2: BTreeMap<SpenderKey, Permit2Allowance>,
}

impl ApprovalSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allowance(&self, key: &ContractAddrKey, spender: &Spender) -> Option<&AllowanceSpec> {
        self.erc20.get(key).and_then(|m| m.get(spender))
    }

    pub fn has_set_for_all(&self, key: &ContractAddrKey, spender: &Spender) -> bool {
        self.set_for_all
            .get(key)
            .map(|s| s.contains(spender))
            .unwrap_or(false)
    }

    pub fn permit2_of(&self, key: &SpenderKey) -> Option<&Permit2Allowance> {
        self.permit2.get(key)
    }
}
