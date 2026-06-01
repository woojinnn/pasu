//! `RepayAction` — repay an outstanding debt position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::{RateMode, TokenRef};
use simulation_state::LiveField;

use super::{LendingVenue, ReserveState, UserLendingState};

/// Repay an outstanding debt position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RepayAction {
    /// Lending venue.
    pub venue: LendingVenue,
    /// Asset being repaid.
    pub asset: TokenRef,
    /// Amount to repay; `U256::MAX` = full repay.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Rate mode of the debt being repaid.
    pub rate_mode: RateMode,
    /// Debtor of record; defaults to `submitter` when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
    /// `Aave V3` flag — repay directly using `aToken` balance.
    pub use_a_tokens: bool,
    /// Live inputs fetched at simulation time.
    pub live_inputs: RepayLiveInputs,
}

/// Live-fetched inputs for a `RepayAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RepayLiveInputs {
    /// Reserve state at simulation time.
    pub reserve_state: LiveField<ReserveState>,
    /// Current outstanding debt for the chosen `RateMode`.
    #[tsify(type = "LiveField<string>")]
    pub current_debt: LiveField<U256>,
    /// User account state before the action.
    pub user_state_before: LiveField<UserLendingState>,
}
