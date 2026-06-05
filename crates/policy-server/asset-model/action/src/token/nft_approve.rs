use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;
use policy_state::token::TokenKey;

/// `ERC721`/`ERC1155` single-token `approve` — grants `spender` rights to a single NFT.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct NftApproveAction {
    /// `TokenKey::Erc721 { .., token_id }` (or `ERC1155` equivalent).
    pub nft_key: TokenKey,
    /// Address authorized to operate the NFT.
    #[tsify(type = "string")]
    pub spender: Address,
}
