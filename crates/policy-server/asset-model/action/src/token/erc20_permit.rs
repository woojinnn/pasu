use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, Time, U256};
use simulation_state::token::TokenRef;
use simulation_state::LiveField;

/// `ERC20` `EIP-2612` `permit` — gasless allowance granted via off-chain signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Erc20PermitAction {
    /// Token whose `permit` is being signed.
    pub token: TokenRef,
    /// Address authorized to spend.
    #[tsify(type = "string")]
    pub spender: Address,
    /// Allowance amount.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Signature expiration timestamp.
    pub deadline: Time,
    /// Current `permit` nonce on the token contract.
    #[tsify(type = "LiveField<string>")]
    pub nonce: LiveField<U256>,
}
