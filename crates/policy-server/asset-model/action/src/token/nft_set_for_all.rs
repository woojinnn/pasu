use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId};

/// `ERC721`/`ERC1155` `setApprovalForAll` — toggles operator status across an entire collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct NftSetForAllAction {
    /// Chain on which the collection lives.
    pub chain: ChainId,
    /// `ERC721` or `ERC1155` contract address.
    #[tsify(type = "string")]
    pub contract: Address,
    /// Operator being granted or revoked.
    #[tsify(type = "string")]
    pub spender: Address,
    /// When `false`, encodes `setApprovalForAll(false)` (revoke).
    pub approved: bool,
}
