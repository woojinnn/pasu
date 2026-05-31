use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId, U256};
use simulation_state::token::{TokenKey, TokenRef};

/// Revoke a previously granted approval, scoped via `RevokeScope`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RevokeApprovalAction {
    /// Which approval to revoke.
    pub scope: RevokeScope,
}

/// Target scope of a `RevokeApprovalAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RevokeScope {
    /// Revoke an `ERC20` allowance for `spender` on `token`.
    Erc20 {
        /// Token whose allowance is being revoked.
        token: TokenRef,
        /// Spender losing the allowance.
        #[tsify(type = "string")]
        spender: Address,
    },
    /// Revoke approval on a single `ERC721`/`ERC1155` token id.
    NftSingleToken {
        /// `TokenKey` identifying the specific NFT.
        nft_key: TokenKey,
    },
    /// Revoke a collection-wide `setApprovalForAll` operator grant.
    NftSetForAll {
        /// Chain on which the collection lives.
        chain: ChainId,
        /// `ERC721`/`ERC1155` contract address.
        #[tsify(type = "string")]
        contract: Address,
        /// Operator losing approval.
        #[tsify(type = "string")]
        spender: Address,
    },
    /// `Permit2` lockdown — revoke a `spender`'s rights on the `Uniswap` `Permit2` contract.
    Permit2Lockdown {
        /// Underlying token whose `Permit2` allowance is being revoked.
        token: TokenRef,
        /// Spender losing the `Permit2` allowance.
        #[tsify(type = "string")]
        spender: Address,
    },
    /// `Permit2` unordered nonce bitmap invalidation.
    Permit2UnorderedNonce {
        /// Chain on which the canonical `Permit2` contract lives.
        chain: ChainId,
        /// Permit2 unordered nonce word position.
        #[tsify(type = "string")]
        word_pos: U256,
        /// Bit mask of unordered nonces invalidated in `word_pos`.
        #[tsify(type = "string")]
        mask: U256,
    },
}
