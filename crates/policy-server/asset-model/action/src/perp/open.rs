//! `OpenPerpAction` — open a new perpetual position at market price.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::{MarginMode, PerpSide};
use policy_state::primitives::{Decimal, MarketRef, Price, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use super::{PerpAccountState, PerpVenue, SizeSpec};

/// Open a new perpetual position at market price.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct OpenPerpAction {
    /// Perpetual venue (e.g. `Hyperliquid`, `GmxV2`).
    pub venue: PerpVenue,
    /// Market symbol (e.g. `ETH-USD`).
    pub market: MarketRef,
    /// Long or short (`PerpSide`).
    pub side: PerpSide,
    /// Position size (`SizeSpec` lets caller pick base / quote / leverage-implied).
    pub size: SizeSpec,
    /// Leverage multiplier to use for this position.
    pub leverage: Decimal,
    /// Collateral token and amount posted.
    #[tsify(type = "[TokenRef, string]")]
    pub collateral: (TokenRef, U256),
    /// Cross or isolated `MarginMode`.
    pub margin_mode: MarginMode,
    /// Maximum acceptable slippage in basis points.
    pub slippage_bp: u32,
    /// If `true`, the order may only reduce existing exposure.
    pub reduce_only: bool,
    /// Live market / account inputs required by the reducer.
    pub live_inputs: OpenPerpLiveInputs,
}

/// Live inputs read at execution time for `OpenPerpAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct OpenPerpLiveInputs {
    /// Venue mark `Price` for the market.
    pub mark_price: LiveField<Price>,
    /// Oracle `Price` for the market.
    pub oracle_price: LiveField<Price>,
    /// Current funding rate (e.g. 1h or 8h).
    pub funding_rate: LiveField<Decimal>,
    /// Remaining venue/market open-interest (OI) capacity.
    #[tsify(type = "LiveField<string>")]
    pub available_oi: LiveField<U256>,
    /// Maximum leverage allowed by the venue/market.
    pub max_leverage: LiveField<Decimal>,
    /// Initial margin requirement in basis points.
    pub initial_margin_bp: LiveField<u32>,
    /// Maintenance margin requirement in basis points.
    pub maintenance_bp: LiveField<u32>,
    /// Taker fee in basis points.
    pub fee_taker_bp: LiveField<u32>,
    /// Maker fee in basis points.
    pub fee_maker_bp: LiveField<u32>,
    /// Current `PerpAccountState` for the user.
    pub user_account_state: LiveField<PerpAccountState>,
}
