//! `DelegateBorrowAction` — `Aave` credit delegation.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::{RateMode, TokenRef};

use super::LendingVenue;

/// `Aave` credit-delegation: authorize another address to borrow on behalf of the submitter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DelegateBorrowAction {
    /// Lending venue (`Aave V2` / `Aave V3`).
    pub venue: LendingVenue,
    /// Asset whose borrow allowance is being delegated.
    pub asset: TokenRef,
    /// Address being granted the borrow allowance.
    #[tsify(type = "string")]
    pub delegatee: Address,
    /// Allowance amount (asset units).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Rate mode covered by the delegation.
    pub rate_mode: RateMode,
}
