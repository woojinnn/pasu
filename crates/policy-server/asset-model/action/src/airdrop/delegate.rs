use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

/// Delegate governance voting power of a governance token (e.g. UNI, COMP, ENS) to another address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DelegateGovernanceAction {
    /// Governance token whose voting power is being delegated (e.g. UNI, COMP, ENS).
    pub token: TokenRef,
    /// Address receiving the delegated voting power.
    #[tsify(type = "string")]
    pub delegatee: Address,
    /// Live-fetched delegation state (current delegate, voting power).
    pub live_inputs: DelegateLiveInputs,
}

/// Live-fetched inputs for a `DelegateGovernanceAction` — current delegation state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DelegateLiveInputs {
    /// Address currently delegated to, if any.
    #[tsify(type = "LiveField<string | null>")]
    pub current_delegate: LiveField<Option<Address>>,
    /// Current voting power held by the delegator.
    #[tsify(type = "LiveField<string>")]
    pub voting_power: LiveField<U256>,
}
