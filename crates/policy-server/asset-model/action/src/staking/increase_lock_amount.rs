//! `IncreaseLockAmountAction` — add tokens to an existing vote-escrow lock.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::StakeVenue;

/// Add `amount` of the governance token to an existing vote-escrow lock,
/// without changing the unlock time.
///
/// Models Curve `VotingEscrow.increase_amount(uint256 _value)` and
/// `deposit_for(address _addr, uint256 _value)`. `on_behalf_of` is the
/// beneficiary whose lock is topped up (`deposit_for._addr`); omitted ⇒ the
/// submitter tops up their own lock (`increase_amount`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct IncreaseLockAmountAction {
    /// Staking / vote-escrow venue (e.g. Curve `VotingEscrow`).
    pub venue: StakeVenue,
    /// Token being added to the lock (e.g. CRV).
    pub token: TokenRef,
    /// Amount of `token` added (`_value`, wei), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Beneficiary whose lock is increased (`deposit_for._addr`); omitted ⇒ submitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
}
