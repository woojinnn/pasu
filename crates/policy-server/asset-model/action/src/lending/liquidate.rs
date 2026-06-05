//! `LiquidateAction` — liquidate an unhealthy position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Price, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use super::{LendingVenue, UserLendingState};

/// Liquidate an unhealthy borrower; typically not invoked from a user wallet, included for completeness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LiquidateAction {
    /// Lending venue.
    pub venue: LendingVenue,
    /// Borrower being liquidated.
    #[tsify(type = "string")]
    pub victim: Address,
    /// Debt asset being repaid by the liquidator.
    pub debt_asset: TokenRef,
    /// Collateral asset being seized.
    pub collat_asset: TokenRef,
    /// Debt amount the liquidator covers.
    #[tsify(type = "string")]
    pub debt_to_cover: U256,
    /// `Aave V3` option — receive seized collateral as `aToken` instead of underlying.
    pub receive_a_token: bool,
    /// Live inputs fetched at simulation time.
    pub live_inputs: LiquidateLiveInputs,
}

/// Live-fetched inputs for a `LiquidateAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LiquidateLiveInputs {
    /// Account state of the borrower being liquidated.
    pub victim_state: LiveField<UserLendingState>,
    /// Liquidation bonus, in basis points.
    pub liquidation_bonus: LiveField<u32>,
    /// Debt asset price in USD.
    pub debt_asset_price: LiveField<Price>,
    /// Collateral asset price in USD.
    pub collat_asset_price: LiveField<Price>,
}
