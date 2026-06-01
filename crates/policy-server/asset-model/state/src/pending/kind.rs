//! `PendingKind` — the four shapes of signature-only / unsettled pending entries.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::position::PerpSide;
use crate::primitives::{Address, MarketRef, Price, Time, VenueRef, U256};
use crate::token::TokenRef;

/// Kind of off-chain-matched order (`UniswapX` / `CowSwap` / 1inch Fusion, etc.).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    /// Dutch auction order whose price decays over time (e.g. `UniswapX`).
    Dutch,
    /// 단순 limit order.
    Limit,
    /// Request-for-Quote (1inch Fusion, Bebop 등).
    Rfq,
}

/// Perp venue 의 미체결 주문 종류.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PerpOrderKind {
    /// 가격 도달 시 체결되는 limit 주문.
    Limit,
    /// trigger 가격 도달 시 시장가 체결.
    StopMarket,
    /// trigger 가격 도달 시 limit 주문 활성.
    StopLimit,
    /// take-profit trigger.
    TakeProfit,
}

/// 서명-only pending entry 의 sub-kind (4 형태).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingKind {
    /// Off-chain-matched limit order (`UniswapX`, `CowSwap`, 1inch Fusion, Bebop, OKX RFQ, etc.).
    OffchainLimitOrder {
        /// 주문을 매칭할 venue.
        venue: VenueRef,
        /// 매도 토큰.
        sell: TokenRef,
        /// 매수 토큰.
        buy: TokenRef,
        /// 매도 가능 최대 양 (base unit).
        #[tsify(type = "string")]
        sell_max: U256,
        /// 매수 받을 최소 양 (base unit).
        #[tsify(type = "string")]
        buy_min: U256,
        /// 주문 종류 (Dutch / Limit / RFQ).
        order_kind: OrderKind,
    },

    /// `PerpDEX` 미체결 리밋.
    PerpVenueOrder {
        /// 주문이 등록된 venue.
        venue: VenueRef,
        /// 거래 market.
        market: MarketRef,
        /// 주문 방향 (Long / Short).
        side: PerpSide,
        /// base asset 단위의 주문 size.
        #[tsify(type = "string")]
        size_base: U256,
        /// 주문 가격.
        price: Price,
        /// 주문 종류 (Limit / Stop / TP).
        order_kind: PerpOrderKind,
        /// reduce-only flag (포지션 늘리기 금지).
        reduce_only: bool,
    },

    /// 서명만 발급된 Permit2 — 잠재적 spend cap.
    SignedPermit2 {
        /// 권한이 부여된 토큰.
        token: TokenRef,
        /// 권한을 받는 spender 주소.
        #[tsify(type = "string")]
        spender: Address,
        /// 한도 양 (base unit).
        #[tsify(type = "string")]
        amount: U256,
        /// 본 권한의 만료 시각.
        expires_at: Time,
        /// Permit2 비트맵 nonce — (word, bit).
        #[tsify(type = "[string, number]")]
        nonce: (U256, u8),
    },

    /// EIP-2612 (USDC, DAI 등).
    SignedEIP2612 {
        /// 권한이 부여된 토큰.
        token: TokenRef,
        /// 권한을 받는 spender 주소.
        #[tsify(type = "string")]
        spender: Address,
        /// 한도 양 (base unit).
        #[tsify(type = "string")]
        amount: U256,
        /// 본 권한의 만료 시각.
        expires_at: Time,
        /// 본 token 의 owner-level nonce.
        #[tsify(type = "string")]
        nonce: U256,
    },
}
