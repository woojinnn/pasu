use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

/// `ERC20` `transfer(recipient, amount)` — direct token transfer from the actor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Erc20TransferAction {
    /// Token being transferred.
    pub token: TokenRef,
    /// Address receiving the tokens.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Amount to transfer.
    #[tsify(type = "string")]
    pub amount: U256,
}
