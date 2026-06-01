use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenKey;

/// `ERC721`/`ERC1155` transfer of a specific token id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct NftTransferAction {
    /// `TokenKey::Erc721` or `TokenKey::Erc1155`.
    pub nft_key: TokenKey,
    /// `ERC1155` quantity; `None` for `ERC721` (implicitly `1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub amount: Option<U256>,
    /// Address receiving the NFT.
    #[tsify(type = "string")]
    pub recipient: Address,
}
