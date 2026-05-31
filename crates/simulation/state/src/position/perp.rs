//! `PerpPosition` — Hyperliquid / GMX V2 / dYdX V4 등의 오픈 포지션.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, Price, SignedI256, VenueRef, U256};
use crate::token::TokenRef;

/// 포지션 방향.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PerpSide {
    /// 가격 상승에 베팅하는 매수 포지션.
    Long,
    /// 가격 하락에 베팅하는 매도 포지션.
    Short,
}

/// margin 적용 방식.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum MarginMode {
    /// 포지션 단위로 격리된 margin.
    Isolated,
    /// 한 계정의 모든 포지션이 margin pool 을 공유.
    Cross,
}

/// 한 perp venue 의 오픈 포지션 + 라이브 mark / liq / pnl / funding / leverage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PerpPosition {
    /// 본 포지션을 호스팅하는 venue.
    pub venue: VenueRef,
    /// 거래 market.
    pub market: MarketRef,
    /// 포지션 방향 (Long / Short).
    pub side: PerpSide,
    /// base asset 단위의 포지션 size.
    #[tsify(type = "string")]
    pub size_base: U256,
    /// USD 환산 notional (entry price × size).
    #[tsify(type = "string")]
    pub notional_usd: U256,
    /// margin 으로 deposit 된 자산 list.
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub collateral: Vec<(TokenRef, U256)>,
    /// 본 포지션의 entry (average) price.
    pub entry_price: Price,
    /// margin 적용 방식 (Isolated / Cross).
    pub margin_mode: MarginMode,

    /// 본 포지션의 mark price (실시간).
    pub mark_price: LiveField<Price>,
    /// 청산 price. 산출 불가 venue 는 inner `None`.
    pub liq_price: LiveField<Option<Price>>,
    /// 미실현 손익 (base unit, 부호 있음).
    #[tsify(type = "LiveField<string>")]
    pub unrealized_pnl: LiveField<SignedI256>,
    /// 미수령 / 미납 funding (부호 있음).
    #[tsify(type = "LiveField<string>")]
    pub funding_owed: LiveField<SignedI256>,
    /// 본 포지션의 실효 leverage.
    pub leverage: LiveField<Decimal>,
}
