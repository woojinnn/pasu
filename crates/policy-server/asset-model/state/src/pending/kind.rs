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
    /// Plain limit order.
    Limit,
    /// Request-for-Quote order, such as 1inch Fusion or Bebop.
    Rfq,
}

/// Unsettled perp order kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PerpOrderKind {
    /// Limit order that fills when price reaches the limit.
    Limit,
    /// Market order that fires when the trigger price is reached.
    StopMarket,
    /// Limit order activated when the trigger price is reached.
    StopLimit,
    /// take-profit trigger.
    TakeProfit,
}

/// Sub-kind for signature-only or unsettled pending entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingKind {
    /// Off-chain-matched limit order (`UniswapX`, `CowSwap`, 1inch Fusion, Bebop, OKX RFQ, etc.).
    OffchainLimitOrder {
        /// Venue that will match the order.
        venue: VenueRef,
        /// Token being sold.
        sell: TokenRef,
        /// Token being bought.
        buy: TokenRef,
        /// Maximum sell amount in base units.
        #[tsify(type = "string")]
        sell_max: U256,
        /// Minimum buy amount in base units.
        #[tsify(type = "string")]
        buy_min: U256,
        /// Order kind.
        order_kind: OrderKind,
    },

    /// Unsettled perp venue order.
    PerpVenueOrder {
        /// Venue where the order is registered.
        venue: VenueRef,
        /// Trading market.
        market: MarketRef,
        /// Order side.
        side: PerpSide,
        /// Order size in base-asset units.
        #[tsify(type = "string")]
        size_base: U256,
        /// Order price.
        price: Price,
        /// Order kind.
        order_kind: PerpOrderKind,
        /// Reduce-only flag.
        reduce_only: bool,
    },

    /// Signed Permit2 allowance that may become a spend cap.
    SignedPermit2 {
        /// Token being authorized.
        token: TokenRef,
        /// Authorized spender address.
        #[tsify(type = "string")]
        spender: Address,
        /// Allowance amount in base units.
        #[tsify(type = "string")]
        amount: U256,
        /// Allowance expiration timestamp.
        expires_at: Time,
        /// Permit2 bitmap nonce as `(word, bit)`.
        #[tsify(type = "[string, number]")]
        nonce: (U256, u8),
    },

    /// Signed EIP-2612 permit, such as USDC or DAI.
    SignedEIP2612 {
        /// Token being authorized.
        token: TokenRef,
        /// Authorized spender address.
        #[tsify(type = "string")]
        spender: Address,
        /// Allowance amount in base units.
        #[tsify(type = "string")]
        amount: U256,
        /// Allowance expiration timestamp.
        expires_at: Time,
        /// Owner-level nonce for this token.
        #[tsify(type = "string")]
        nonce: U256,
    },
}
