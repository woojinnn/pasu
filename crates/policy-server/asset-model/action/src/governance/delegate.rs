//! `GovernanceDelegateAction` — delegate governance voting/proposition power.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use super::{GovernanceDelegationKind, GovernanceVenue};

/// Delegate governance voting/proposition power of a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceDelegateAction {
    /// Governance venue.
    pub venue: GovernanceVenue,
    /// Governance token whose power is being delegated.
    pub token: TokenRef,
    /// Address receiving delegated power.
    #[tsify(type = "string")]
    pub delegatee: Address,
    /// Delegation kind.
    pub delegation_kind: GovernanceDelegationKind,
    /// Raw on-chain delegation type when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub raw_delegation_type: Option<u8>,
    /// Live-fetched delegation state.
    pub live_inputs: GovernanceDelegateLiveInputs,
}

/// Live-fetched inputs for governance delegation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceDelegateLiveInputs {
    /// Current delegate, when known.
    #[tsify(type = "LiveField<string | null>")]
    pub current_delegate: LiveField<Option<Address>>,
    /// Current governance power.
    #[tsify(type = "LiveField<string>")]
    pub governance_power: LiveField<U256>,
}
