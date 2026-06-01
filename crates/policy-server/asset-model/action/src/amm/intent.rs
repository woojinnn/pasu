//! Intent-based off-chain signed orders — `SignIntentOrder` / `CancelIntentOrder`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId, Price, Time, U256};
use simulation_state::token::TokenRef;
use simulation_state::LiveField;

use crate::Bytes;

/// Sign an EIP-712 intent order (`UniswapX` Dutch, `CowSwap` limit, `1inch Fusion` RFQ, ...).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SignIntentOrderAction {
    /// Intent venue receiving the signed order.
    pub venue: IntentVenue,
    /// Token being sold.
    pub sell: TokenRef,
    /// Token being bought.
    pub buy: TokenRef,
    /// Amount of `sell` token offered.
    #[tsify(type = "string")]
    pub sell_amount: U256,
    /// Minimum acceptable amount of `buy` token.
    #[tsify(type = "string")]
    pub buy_min: U256,
    /// Order semantics (Dutch / Limit / RFQ).
    pub order_kind: IntentOrderKind,
    /// Recipient of the buy token when the order fills.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Order expiry timestamp.
    pub valid_until: Time,
    /// Simulation-time inputs (expected fill price, competing-order count).
    pub live_inputs: SignIntentOrderLiveInputs,
}

/// Off-chain intent-order venue (EIP-712 signed limit / Dutch / RFQ orders).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum IntentVenue {
    /// `UniswapX` reactor-based Dutch / limit orders.
    UniswapX {
        /// Chain the reactor lives on.
        chain: ChainId,
        /// `UniswapX` reactor contract.
        #[tsify(type = "string")]
        reactor: Address,
    },
    /// `CoW Swap` batch settlement.
    CowSwap {
        /// Chain the settlement contract lives on.
        chain: ChainId,
        /// `CoW Swap` `GPv2Settlement` contract.
        #[tsify(type = "string")]
        settlement: Address,
    },
    /// `1inch Fusion` resolver-based orders.
    OneInchFusion {
        /// Chain the Fusion order is bound to.
        chain: ChainId,
    },
    /// `Bebop` RFQ orders.
    Bebop {
        /// Chain the Bebop order is bound to.
        chain: ChainId,
    },
}

impl IntentVenue {
    /// The venue's `serde` `name` tag (e.g. `"uniswap_x"`, `"one_inch_fusion"`).
    ///
    /// These strings match the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminants exactly and are verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::UniswapX { .. } => "uniswap_x",
            Self::CowSwap { .. } => "cow_swap",
            Self::OneInchFusion { .. } => "one_inch_fusion",
            Self::Bebop { .. } => "bebop",
        }
    }
}

/// Semantics of an intent order's price discovery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum IntentOrderKind {
    /// Dutch auction (price decays over time).
    Dutch,
    /// Fixed limit order.
    Limit,
    /// Request-for-quote (solver-/maker-quoted) order.
    Rfq,
}

/// Simulation-time inputs for a `SignIntentOrder` action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SignIntentOrderLiveInputs {
    /// Expected fill price at simulation time.
    pub expected_fill_price: LiveField<Price>,
    /// Number of active competing orders on the same pair.
    pub competing_orders: LiveField<u32>,
}

/// Cancel a previously signed intent order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CancelIntentOrderAction {
    /// Intent venue the order was signed against.
    pub venue: IntentVenue,
    /// 32-byte hex order hash being cancelled.
    pub order_hash: String,
    /// Some venues use an EIP-712 signature to authorize the cancellation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub signature: Option<Bytes>,
}
