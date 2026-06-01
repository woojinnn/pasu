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
    /// Standard limit order filled at or better than a fixed price.
    Limit,
    /// Request-for-quote order matched against a market-maker quote.
    Rfq,
}

/// Kind of resting (unfilled) order on a perpetual-futures venue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PerpOrderKind {
    /// Limit order resting at a fixed price.
    Limit,
    /// Stop order that triggers a market order once the stop price is reached.
    StopMarket,
    /// Stop order that triggers a limit order once the stop price is reached.
    StopLimit,
    /// Take-profit order that closes the position at a favorable target price.
    TakeProfit,
}

/// A signature-only or unsettled entry that may still consume funds or open a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingKind {
    /// Off-chain-matched limit order (`UniswapX`, `CowSwap`, 1inch Fusion, Bebop, OKX RFQ, etc.).
    OffchainLimitOrder {
        /// Venue the order was submitted to.
        venue: VenueRef,
        /// Token being sold (input side of the swap).
        sell: TokenRef,
        /// Token being bought (output side of the swap).
        buy: TokenRef,
        /// Maximum amount of `sell` token spendable (raw token units).
        #[tsify(type = "string")]
        sell_max: U256,
        /// Minimum amount of `buy` token to receive (raw token units).
        #[tsify(type = "string")]
        buy_min: U256,
        /// Matching style of the order (Dutch / limit / RFQ).
        order_kind: OrderKind,
    },

    /// Unfilled resting order on a perpetual-futures DEX.
    PerpVenueOrder {
        /// Perp venue the order rests on.
        venue: VenueRef,
        /// Market (trading pair) the order targets.
        market: MarketRef,
        /// Position side the order would take (long / short).
        side: PerpSide,
        /// Order size denominated in the base asset (raw units).
        #[tsify(type = "string")]
        size_base: U256,
        /// Limit / trigger price of the order.
        price: Price,
        /// Order kind (limit / stop-market / stop-limit / take-profit).
        order_kind: PerpOrderKind,
        /// Whether the order may only reduce, never increase, the position.
        reduce_only: bool,
    },

    /// A signed-but-unused Permit2 approval — a potential spend cap.
    SignedPermit2 {
        /// Token the approval applies to.
        token: TokenRef,
        /// Address authorized to spend the token.
        #[tsify(type = "string")]
        spender: Address,
        /// Maximum amount the spender is permitted to transfer (raw token units).
        #[tsify(type = "string")]
        amount: U256,
        /// Timestamp at which the permit expires.
        expires_at: Time,
        /// Permit2 unordered nonce as a `(word, bit)` pair.
        #[tsify(type = "[string, number]")]
        nonce: (U256, u8), // (word, bit)
    },

    /// A signed-but-unused EIP-2612 `permit` approval (USDC, DAI, etc.).
    SignedEIP2612 {
        /// Token the approval applies to.
        token: TokenRef,
        /// Address authorized to spend the token.
        #[tsify(type = "string")]
        spender: Address,
        /// Maximum amount the spender is permitted to transfer (raw token units).
        #[tsify(type = "string")]
        amount: U256,
        /// Timestamp at which the permit expires.
        expires_at: Time,
        /// Sequential EIP-2612 nonce for the token owner.
        #[tsify(type = "string")]
        nonce: U256,
    },
}
