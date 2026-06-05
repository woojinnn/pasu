//! `WithdrawAction` — withdraw a previously supplied asset.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use super::{LendingVenue, ReserveState, UserLendingState};

/// Withdraw a previously supplied asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WithdrawAction {
    /// Lending venue.
    pub venue: LendingVenue,
    /// Asset being withdrawn.
    pub asset: TokenRef,
    /// Amount to withdraw; `U256::MAX` = max-withdraw.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Address receiving the withdrawn funds.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Live inputs fetched at simulation time.
    pub live_inputs: WithdrawLiveInputs,
}

/// Live-fetched inputs for a `WithdrawAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WithdrawLiveInputs {
    /// Reserve state at simulation time.
    pub reserve_state: LiveField<ReserveState>,
    /// Maximum amount the user can withdraw right now.
    #[tsify(type = "LiveField<string>")]
    pub available_to_withdraw: LiveField<U256>,
    /// User account state before the action.
    pub user_state_before: LiveField<UserLendingState>,
}
