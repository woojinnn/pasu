//! `ChangeLeverageAction` — change the leverage setting for a market.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::position::PositionId;
use simulation_state::primitives::{Decimal, MarketRef, Price};
use simulation_state::LiveField;

use super::PerpVenue;

/// Change the leverage setting for a market.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ChangeLeverageAction {
    /// Perpetual venue on which leverage is being changed.
    pub venue: PerpVenue,
    /// Market the new leverage applies to.
    pub market: MarketRef,
    /// New leverage multiplier.
    pub new_leverage: Decimal,
    /// Live venue / position inputs.
    pub live_inputs: ChangeLeverageLiveInputs,
}

/// Live inputs read at execution time for `ChangeLeverageAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ChangeLeverageLiveInputs {
    /// Maximum leverage allowed by the venue/market.
    pub max_leverage: LiveField<Decimal>,
    /// Positions affected by the leverage change.
    pub affected_positions: LiveField<Vec<PositionId>>,
    /// New liquidation `Price` for each affected position.
    pub new_liq_prices: LiveField<Vec<(PositionId, Option<Price>)>>,
}
