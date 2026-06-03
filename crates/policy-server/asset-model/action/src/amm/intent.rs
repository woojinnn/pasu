//! Intent-based off-chain signed orders — `SignIntentOrder` /
//! `SettleIntentOrder` / `CancelIntentOrder`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId, Price, Time, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

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

/// Submit an on-chain fill/settlement transaction for a previously signed
/// intent order. The actor may be the swapper or a third-party filler; the
/// action therefore records the signed order's economic terms without
/// pretending the submitter is necessarily the seller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SettleIntentOrderAction {
    /// Intent venue whose settlement contract is being called.
    pub venue: IntentVenue,
    /// Address that originally signed/created the order.
    #[tsify(type = "string")]
    pub swapper: Address,
    /// Token the signed order sells.
    pub sell: TokenRef,
    /// Token the signed order buys.
    pub buy: TokenRef,
    /// Sell-side amount/cap decoded from the signed order.
    #[tsify(type = "string")]
    pub sell_amount: U256,
    /// Minimum acceptable buy-side amount decoded from the signed order.
    #[tsify(type = "string")]
    pub buy_min: U256,
    /// Order semantics (Dutch / Limit / RFQ).
    pub order_kind: IntentOrderKind,
    /// Recipient of the buy token when the order fills.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Order expiry timestamp.
    pub valid_until: Time,
    /// Venue-side order nonce.
    #[tsify(type = "string")]
    pub order_nonce: U256,
    /// Signature submitted with the settlement transaction, when surfaced by
    /// the calldata route. Stored for audit; verification is an upstream
    /// adapter/orchestrator responsibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub signature: Option<Bytes>,
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
    /// `1inch Limit Order Protocol` (LOP v4) maker-signed limit orders. Distinct
    /// from [`Self::OneInchFusion`] (RFQ / Dutch-resolver): a plain signed limit
    /// order with on-chain maker cancellation. Carries the EIP-712 verifying
    /// contract — the `AggregationRouterV6` the LOP v4 is embedded in.
    OneInchLimitOrder {
        /// Chain the LOP order is bound to.
        chain: ChainId,
        /// EIP-712 verifying contract (`AggregationRouterV6`).
        #[tsify(type = "string")]
        verifying_contract: Address,
    },
}

impl IntentVenue {
    /// The venue's `serde` `name` tag (e.g. `"uniswap_x"`, `"one_inch_fusion"`).
    /// These strings match the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminants exactly and are verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::UniswapX { .. } => "uniswap_x",
            Self::CowSwap { .. } => "cow_swap",
            Self::OneInchFusion { .. } => "one_inch_fusion",
            Self::Bebop { .. } => "bebop",
            Self::OneInchLimitOrder { .. } => "one_inch_limit_order",
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

/// Set (or revoke) an **on-chain pre-signature** for an intent order — CoW
/// Protocol's `GPv2Settlement.setPreSignature(bytes orderUid, bool signed)`.
///
/// This is the smart-contract-wallet order-placement path: an EOA signs the
/// EIP-712 `Order` off-chain (→ [`SignIntentOrderAction`]), whereas a contract
/// wallet (e.g. a Safe) authorises the *same* order on-chain by marking its
/// `orderUid` pre-signed. The calldata carries only the opaque `orderUid`
/// (56 bytes = 32-byte orderDigest ‖ 20-byte owner ‖ 4-byte validTo, per
/// `GPv2Order.packOrderUidParams`) and the `signed` flag — the order's
/// economic terms (sell / buy / amounts) live in the off-chain order keyed by
/// the digest and are **not** statically decodable here (the CoW orderbook
/// API is the enrichment source). We therefore carry the opaque `order_hash`
/// together with the `signed` direction and leave the terms unmodelled rather
/// than fabricate them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PreSignIntentOrderAction {
    /// Intent venue the pre-signed order belongs to (CoW Swap settlement).
    pub venue: IntentVenue,
    /// Opaque order identifier being (de)authorised. For CoW Swap this is the
    /// 56-byte `orderUid` hex (orderDigest ‖ owner ‖ validTo).
    pub order_hash: String,
    /// `true` marks the order tradable (pre-signed / commit); `false` revokes
    /// a prior pre-signature (cancel).
    pub signed: bool,
}
