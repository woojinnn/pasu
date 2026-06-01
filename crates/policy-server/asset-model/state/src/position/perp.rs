//! `PerpPosition` — an open perpetual-futures position on venues such as
//! Hyperliquid, GMX V2, or dYdX V4.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, Price, SignedI256, VenueRef, U256};
use crate::token::TokenRef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
/// Directional side of a perpetual position.
pub enum PerpSide {
    /// Long position that profits when the market price rises.
    Long,
    /// Short position that profits when the market price falls.
    Short,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
/// How margin is allocated to back the position.
pub enum MarginMode {
    /// Isolated margin: collateral is dedicated to this single position.
    Isolated,
    /// Cross margin: collateral is shared across the account's positions.
    Cross,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// A single open perpetual-futures position held on a trading venue.
pub struct PerpPosition {
    /// Trading venue that hosts this position (e.g. Hyperliquid, GMX V2).
    pub venue: VenueRef,
    /// Market the position is opened in (e.g. "ETH-USD").
    pub market: MarketRef,
    /// Whether the position is long or short.
    pub side: PerpSide,
    /// Position size denominated in the base asset (raw integer units).
    #[tsify(type = "string")]
    pub size_base: U256,
    /// Notional value of the position in USD (raw integer units).
    #[tsify(type = "string")]
    pub notional_usd: U256,
    /// Collateral backing the position, as (token, raw amount) pairs.
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub collateral: Vec<(TokenRef, U256)>,
    /// Average entry price at which the position was opened.
    pub entry_price: Price,
    /// Margin allocation mode (isolated or cross).
    pub margin_mode: MarginMode,

    /// Current mark price used for valuation and liquidation checks.
    pub mark_price: LiveField<Price>,
    /// Estimated liquidation price, or `None` when not applicable/unknown.
    pub liq_price: LiveField<Option<Price>>,
    /// Unrealized profit/loss of the position (signed, raw integer units).
    #[tsify(type = "LiveField<string>")]
    pub unrealized_pnl: LiveField<SignedI256>,
    /// Funding currently owed by (negative) or to (positive) the position.
    #[tsify(type = "LiveField<string>")]
    pub funding_owed: LiveField<SignedI256>,
    /// Effective leverage of the position (notional / collateral).
    pub leverage: LiveField<Decimal>,
}
