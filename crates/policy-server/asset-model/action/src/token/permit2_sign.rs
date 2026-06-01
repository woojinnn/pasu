use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, Time, U256};
use simulation_state::token::TokenRef;
use simulation_state::LiveField;

/// `Uniswap` `Permit2` signed allowance — off-chain signature consumed by `Permit2`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2SignAction {
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
    /// Timestamp at which the signature itself expires.
    pub sig_deadline: Time,
    /// `(word, bit)` pair — `Permit2` nonce bitmap coordinates.
    #[tsify(type = "LiveField<[string, number]>")]
    pub nonce: LiveField<(U256, u8)>,
}
