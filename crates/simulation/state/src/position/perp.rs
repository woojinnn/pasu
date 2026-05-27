//! PerpPosition — Hyperliquid / GMX V2 / dYdX V4 등의 오픈 포지션.

use serde::{Deserialize, Serialize};

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, Price, SignedI256, U256, VenueRef};
use crate::token::TokenRef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerpSide {
    Long,
    Short,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarginMode {
    Isolated,
    Cross,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpPosition {
    pub venue: VenueRef,
    pub market: MarketRef,
    pub side: PerpSide,
    pub size_base: U256,
    pub notional_usd: U256,
    pub collateral: Vec<(TokenRef, U256)>,
    pub entry_price: Price,
    pub margin_mode: MarginMode,

    pub mark_price: LiveField<Price>,
    pub liq_price: LiveField<Option<Price>>,
    pub unrealized_pnl: LiveField<SignedI256>,
    pub funding_owed: LiveField<SignedI256>,
    pub leverage: LiveField<Decimal>,
}
