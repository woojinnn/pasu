//! `SwapRateModeAction` — switch the rate mode of an existing `Aave` debt position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Decimal, U256};
use simulation_state::token::{RateMode, TokenRef};
use simulation_state::LiveField;

use super::LendingVenue;

/// Switch the rate mode of an existing `Aave` debt position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SwapRateModeAction {
    /// Lending venue (`Aave V2` / `Aave V3`).
    pub venue: LendingVenue,
    /// Asset whose debt rate mode is being switched.
    pub asset: TokenRef,
    /// Target rate mode after the swap.
    pub new_mode: RateMode,
    /// Live inputs fetched at simulation time.
    pub live_inputs: SwapRateModeLiveInputs,
}

/// Live-fetched inputs for a `SwapRateModeAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SwapRateModeLiveInputs {
    /// Current `(variable, stable)` debt balances.
    #[tsify(type = "LiveField<[string, string]>")]
    pub current_debts: LiveField<(U256, U256)>,
    /// Current `(variable, stable)` borrow rates.
    pub rates: LiveField<(Decimal, Decimal)>,
}
