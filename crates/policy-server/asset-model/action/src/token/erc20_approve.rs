use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

/// `ERC20` `approve(spender, amount)` — grants `spender` allowance up to `amount` of `token`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Erc20ApproveAction {
    /// Token being approved.
    pub token: TokenRef,
    /// Address authorized to spend.
    #[tsify(type = "string")]
    pub spender: Address,
    /// Allowance amount; `U256::MAX` means unlimited.
    #[tsify(type = "string")]
    pub amount: U256,
    // No `live_inputs` — fully deterministic.
}
