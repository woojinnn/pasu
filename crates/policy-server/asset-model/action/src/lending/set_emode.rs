//! `SetEModeAction` — select an `Aave V3` e-mode category for the user.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::position::EModeCategory;
use simulation_state::primitives::Address;
use simulation_state::token::TokenRef;
use simulation_state::LiveField;

use super::{LendingVenue, UserLendingState};

/// Select an `Aave V3` e-mode category for the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SetEModeAction {
    /// Lending venue (`Aave V3`).
    pub venue: LendingVenue,
    /// Target category id; `0` = disable e-mode.
    pub category_id: u8,
    /// Account whose e-mode is changed; defaults to `submitter` when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
    /// Live inputs fetched at simulation time.
    pub live_inputs: SetEModeLiveInputs,
}

/// Live-fetched inputs for a `SetEModeAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SetEModeLiveInputs {
    /// Configuration of the target e-mode category.
    pub category_config: LiveField<EModeConfig>,
    /// User account state before the action.
    pub user_state_before: LiveField<UserLendingState>,
}

/// E-mode category configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EModeConfig {
    /// Loan-to-value within the category, in basis points.
    pub ltv_bp: u32,
    /// Liquidation threshold within the category, in basis points.
    pub liquidation_threshold_bp: u32,
    /// Liquidation bonus within the category, in basis points.
    pub liquidation_bonus_bp: u32,
    /// Optional category-specific price source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub price_source: Option<Address>,
    /// Assets eligible under this category.
    pub assets_in_category: Vec<TokenRef>,
    /// `EModeCategory` id from the state crate (reuses `Aave`'s category labels).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub category: Option<EModeCategory>,
}
