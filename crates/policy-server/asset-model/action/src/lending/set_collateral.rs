//! `SetCollateralAction` — enable or disable an asset's use as collateral.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::token::TokenRef;
use simulation_state::LiveField;

use super::{LendingVenue, ReserveState, UserLendingState};

/// Enable or disable an asset's use as collateral.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SetCollateralAction {
    /// Lending venue.
    pub venue: LendingVenue,
    /// Asset whose collateral flag is being toggled.
    pub asset: TokenRef,
    /// Live inputs fetched at simulation time.
    pub live_inputs: SetCollateralLiveInputs,
}

/// Live-fetched inputs for a `SetCollateralAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SetCollateralLiveInputs {
    /// Reserve state at simulation time.
    pub reserve_state: LiveField<ReserveState>,
    /// User account state before the action.
    pub user_state_before: LiveField<UserLendingState>,
}
