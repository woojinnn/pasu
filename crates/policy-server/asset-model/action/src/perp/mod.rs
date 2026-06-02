//! `PerpAction` — `OpenPosition`/`ClosePosition`/`AdjustMargin`/`PlaceLimitOrder`/`PlaceStopOrder`/`CancelOrder`/`ClaimFunding`. See spec §9.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId, Decimal, MarketRef, Price, SignedI256, U256};

pub mod adjust_margin;
pub mod cancel;
pub mod change_leverage;
pub mod change_margin_mode;
pub mod claim_funding;
pub mod close;
pub mod decrease;
pub mod increase;
pub mod open;
pub mod place_limit;
pub mod place_stop;

pub use self::adjust_margin::*;
pub use self::cancel::*;
pub use self::change_leverage::*;
pub use self::change_margin_mode::*;
pub use self::claim_funding::*;
pub use self::close::*;
pub use self::decrease::*;
pub use self::increase::*;
pub use self::open::*;
pub use self::place_limit::*;
pub use self::place_stop::*;

// ---------------------------------------------------------------------------
// Domain enum
// ---------------------------------------------------------------------------

/// Top-level perpetuals action dispatched by the reducer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PerpAction {
    /// Open a new perpetual position.
    OpenPosition(OpenPerpAction),
    /// Close (fully or partially) an existing position.
    ClosePosition(ClosePerpAction),
    /// Add size to an existing position.
    IncreasePosition(IncreasePerpAction),
    /// Reduce size of an existing position without fully closing it.
    DecreasePosition(DecreasePerpAction),
    /// Add or withdraw collateral from a position.
    AdjustMargin(AdjustMarginAction),
    /// Change the leverage setting for a market.
    ChangeLeverage(ChangeLeverageAction),
    /// Cross <-> Isolated margin mode switch.
    ChangeMarginMode(ChangeMarginModeAction),
    /// Place a limit order on the venue's orderbook.
    PlaceLimitOrder(PlaceLimitOrderAction),
    /// `StopMarket` | `StopLimit` | `TakeProfit` | `TakeProfitLimit`.
    PlaceStopOrder(PlaceStopOrderAction),
    /// Cancel a previously placed open order.
    CancelOrder(CancelOrderAction),
    /// Claim accrued funding payments.
    ClaimFunding(ClaimFundingAction),
}

impl PerpAction {
    /// The action's `serde` `action` tag (e.g. `"open_position"`, `"claim_funding"`).
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::OpenPosition(_) => "open_position",
            Self::ClosePosition(_) => "close_position",
            Self::IncreasePosition(_) => "increase_position",
            Self::DecreasePosition(_) => "decrease_position",
            Self::AdjustMargin(_) => "adjust_margin",
            Self::ChangeLeverage(_) => "change_leverage",
            Self::ChangeMarginMode(_) => "change_margin_mode",
            Self::PlaceLimitOrder(_) => "place_limit_order",
            Self::PlaceStopOrder(_) => "place_stop_order",
            Self::CancelOrder(_) => "cancel_order",
            Self::ClaimFunding(_) => "claim_funding",
        }
    }

    /// The venue `name` of the wrapped action. Every perp action carries a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::OpenPosition(a) => Some(a.venue.name()),
            Self::ClosePosition(a) => Some(a.venue.name()),
            Self::IncreasePosition(a) => Some(a.venue.name()),
            Self::DecreasePosition(a) => Some(a.venue.name()),
            Self::AdjustMargin(a) => Some(a.venue.name()),
            Self::ChangeLeverage(a) => Some(a.venue.name()),
            Self::ChangeMarginMode(a) => Some(a.venue.name()),
            Self::PlaceLimitOrder(a) => Some(a.venue.name()),
            Self::PlaceStopOrder(a) => Some(a.venue.name()),
            Self::CancelOrder(a) => Some(a.venue.name()),
            Self::ClaimFunding(a) => Some(a.venue.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Venue
// ---------------------------------------------------------------------------

/// Perpetual trading venue (protocol + chain).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum PerpVenue {
    /// `Hyperliquid` L1 (off-chain orderbook).
    Hyperliquid {
        /// Chain identifier for the `Hyperliquid` L1.
        chain: ChainId,
    },
    /// `GMX V2` perpetual venue.
    GmxV2 {
        /// Chain hosting the `GMX V2` deployment.
        chain: ChainId,
    },
    /// `dYdX V4` — runs on a Cosmos chain.
    DyDxV4 {
        /// Cosmos chain identifier for `dYdX V4`.
        chain: ChainId,
    },
    /// `Vertex` perpetual venue.
    Vertex {
        /// Chain hosting the `Vertex` deployment.
        chain: ChainId,
    },
    /// `Aevo` perpetual venue.
    Aevo {
        /// Chain hosting the `Aevo` deployment.
        chain: ChainId,
    },
    /// `Drift` — on Solana.
    Drift {
        /// Solana chain identifier for `Drift`.
        chain: ChainId,
    },
    /// `Jupiter Perps` — on Solana.
    JupiterPerps {
        /// Solana chain identifier for `Jupiter Perps`.
        chain: ChainId,
    },
    /// `Synthetix` perpetual venue.
    Synthetix {
        /// Chain hosting the `Synthetix` deployment.
        chain: ChainId,
    },
    /// Generic / unspecified perpetual contract.
    Generic {
        /// Chain on which the contract is deployed.
        chain: ChainId,
        /// Address of the perpetual contract.
        #[tsify(type = "string")]
        contract: Address,
    },
}

impl PerpVenue {
    /// The venue's `serde` `name` tag (e.g. `"hyperliquid"`, `"dy_dx_v4"`).
    /// These strings match the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminants exactly and are verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hyperliquid { .. } => "hyperliquid",
            Self::GmxV2 { .. } => "gmx_v2",
            Self::DyDxV4 { .. } => "dy_dx_v4",
            Self::Vertex { .. } => "vertex",
            Self::Aevo { .. } => "aevo",
            Self::Drift { .. } => "drift",
            Self::JupiterPerps { .. } => "jupiter_perps",
            Self::Synthetix { .. } => "synthetix",
            Self::Generic { .. } => "generic",
        }
    }
}

// ---------------------------------------------------------------------------
// Size specification
// ---------------------------------------------------------------------------

/// How the caller specifies position / order size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SizeSpec {
    /// Base asset units (e.g. "1 ETH").
    BaseAmount {
        /// Amount denominated in the base asset.
        #[tsify(type = "string")]
        amount: U256,
    },
    /// Quote (USD) units (e.g. "$5000 worth").
    QuoteAmount {
        /// Amount denominated in USD quote units.
        #[tsify(type = "string")]
        amount_usd: U256,
    },
    /// Derived from collateral * leverage.
    LeverageImplied {
        /// Collateral committed to the position.
        #[tsify(type = "string")]
        collateral: U256,
        /// Leverage multiplier applied to `collateral`.
        leverage: Decimal,
    },
}

// ---------------------------------------------------------------------------
// Order lifecycle options
// ---------------------------------------------------------------------------

/// Order time-in-force policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimeInForce {
    /// Good Till Cancelled.
    Gtc,
    /// Immediate Or Cancel — cancel any unfilled portion immediately.
    Ioc,
    /// Fill Or Kill — cancel the entire order if it cannot be fully filled immediately.
    Fok,
    /// Maker-only — reject if the order would result in a taker fill.
    PostOnly,
    /// Good Till Date — only supported on some venues.
    Gtd {
        /// Expiration time of the order.
        until: policy_state::primitives::Time,
    },
}

/// Kind of stop / take-profit order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum StopOrderKind {
    /// Stop order that executes as a market order once triggered.
    StopMarket,
    /// Stop order that places a limit order once triggered.
    StopLimit,
    /// Take-profit order executed as a market order once triggered.
    TakeProfit,
    /// Take-profit order placed as a limit order once triggered.
    TakeProfitLimit,
}

// ---------------------------------------------------------------------------
// Shared sub-types
// ---------------------------------------------------------------------------

/// Aggregate margin / collateral snapshot for a perp account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PerpAccountState {
    /// Total collateral on the account, in USD.
    #[tsify(type = "string")]
    pub total_collateral_usd: U256,
    /// Margin currently locked by open positions / orders, in USD.
    #[tsify(type = "string")]
    pub used_margin_usd: U256,
    /// Margin available for new positions / orders, in USD.
    #[tsify(type = "string")]
    pub free_margin_usd: U256,
    /// Existing exposure per market.
    #[tsify(type = "Array<[MarketRef, string]>")]
    pub open_positions: Vec<(MarketRef, U256)>,
}

/// Live snapshot of a single perp position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PerpPositionLive {
    /// Position size in base asset units.
    #[tsify(type = "string")]
    pub size_base: U256,
    /// Notional value of the position in USD.
    #[tsify(type = "string")]
    pub notional_usd: U256,
    /// Average entry `Price`.
    pub entry_price: Price,
    /// Current mark `Price` used for `PnL` / liquidation.
    pub mark_price: Price,
    /// Liquidation `Price` if computable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub liq_price: Option<Price>,
    /// Unrealized `PnL` as a `SignedI256` (positive = profit).
    #[tsify(type = "string")]
    pub unrealized_pnl: SignedI256,
}
