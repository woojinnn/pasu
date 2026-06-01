use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, Time, U256};
use simulation_state::token::TokenRef;

/// `Uniswap` `Permit2` on-chain `approve` — sets allowance on the `Permit2` contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2ApproveAction {
    /// Underlying token whose allowance is delegated through `Permit2`.
    pub token: TokenRef,
    /// Address authorized to spend.
    #[tsify(type = "string")]
    pub spender: Address,
    /// Allowance amount.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Timestamp at which the allowance expires.
    pub expires_at: Time,
}
