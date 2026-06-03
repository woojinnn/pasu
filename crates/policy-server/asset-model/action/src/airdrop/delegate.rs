use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

/// Which governance power a delegation transfers.
///
/// ERC20Votes-style tokens (UNI, COMP, ENS) delegate both powers at once with a
/// single `delegate(address)`, which is the default. `Aave`-style governance
/// tokens (AAVE / stkAAVE / aAAVE) split power into voting and proposition and
/// expose `delegateByType(address, uint8)` (0 = voting, 1 = proposition).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum GovernancePowerType {
    /// Both voting and proposition power (ERC20Votes `delegate`, Aave `delegate`/`metaDelegate`).
    #[default]
    VotingAndProposition,
    /// Voting power only (`Aave` `delegateByType(_, 0)`).
    Voting,
    /// Proposition power only (`Aave` `delegateByType(_, 1)`).
    Proposition,
}

/// Delegate governance voting power of a governance token (e.g. UNI, COMP, ENS, AAVE) to another address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DelegateGovernanceAction {
    /// Governance token whose voting power is being delegated (e.g. UNI, COMP, ENS, AAVE).
    pub token: TokenRef,
    /// Address receiving the delegated voting power.
    #[tsify(type = "string")]
    pub delegatee: Address,
    /// Which governance power is delegated. ERC20Votes-style tokens delegate
    /// both at once (the default); `Aave`-style tokens set this from
    /// `delegateByType`. `#[serde(default)]` keeps pre-existing manifests that
    /// omit the field deserializing as `voting_and_proposition`.
    #[serde(default)]
    pub power_type: GovernancePowerType,
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
