//! `PerpPosition` represents open perp positions across venues.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, Price, SignedI256, VenueRef, U256};
use crate::token::TokenRef;

/// Position side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PerpSide {
    /// Long position.
    Long,
    /// Short position.
    Short,
}

/// Margin accounting mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum MarginMode {
    /// Margin isolated to this position.
    Isolated,
    /// Margin shared across the account's positions.
    Cross,
}

/// Open position plus live mark, liquidation, `PnL`, funding, and leverage fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PerpPosition {
    /// Venue hosting this position.
    pub venue: VenueRef,
    /// Trading market.
    pub market: MarketRef,
    /// Position side.
    pub side: PerpSide,
    /// Position size in base-asset units.
    #[tsify(type = "string")]
    pub size_base: U256,
    /// Notional value in USD.
    #[tsify(type = "string")]
    pub notional_usd: U256,
    /// Assets deposited as margin.
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub collateral: Vec<(TokenRef, U256)>,
    /// Average entry price.
    pub entry_price: Price,
    /// Margin accounting mode.
    pub margin_mode: MarginMode,

    /// Live mark price.
    pub mark_price: LiveField<Price>,
    /// Liquidation price; inner `None` when the venue cannot provide one.
    pub liq_price: LiveField<Option<Price>>,
    /// Unrealized `PnL` in signed base units.
    #[tsify(type = "LiveField<string>")]
    pub unrealized_pnl: LiveField<SignedI256>,
    /// Funding owed or receivable in signed base units.
    #[tsify(type = "LiveField<string>")]
    pub funding_owed: LiveField<SignedI256>,
    /// Effective leverage for this position.
    pub leverage: LiveField<Decimal>,
}
