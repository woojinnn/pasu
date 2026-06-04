//! `FulfillOrderAction` — a taker fulfills one or more marketplace orders
//! on-chain (Seaport `fulfill*` / `match*`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;

use super::item::MarketItem;
use super::venue::MarketplaceVenue;
use crate::Bytes;

/// On-chain fulfillment: the taker RECEIVES `offer[]` and PAYS `consideration[]`
/// (incl. fees / royalties). For batch fulfill/match the items are a COARSE
/// concatenation across all orders — Seaport `fulfillments` netting is NOT
/// applied (over-disclosure: gross legs, conservative for a static analyzer).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct FulfillOrderAction {
    /// Settlement venue (Seaport).
    pub venue: MarketplaceVenue,
    /// Items the taker RECEIVES (aggregated across fulfilled orders).
    pub offer: Vec<MarketItem>,
    /// Items the taker PAYS, each to its recipient (aggregated).
    pub consideration: Vec<MarketItem>,
    /// Recipient of the offered items (explicit `recipient` arg, or the submitter).
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Taker's conduit key (bytes32 hex), when present in calldata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub fulfiller_conduit_key: Option<Bytes>,
    /// Number of orders fulfilled in this call (1 for single fulfill).
    pub order_count: u32,
    /// Whether this is a batch fulfill/match (coarse aggregation applied).
    pub is_batch: bool,
}
