//! `CancelOrderAction` — a maker revokes their own orders on-chain
//! (Seaport `cancel(OrderComponents[])` or `incrementCounter()`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::venue::MarketplaceVenue;

/// Revoke marketplace orders. `scope = "orders"` cancels the specific orders in
/// calldata (`order_count` of them); `scope = "all"` is `incrementCounter`
/// (bulk-invalidate every outstanding signed order — Seaport "cancel all
/// listings"). No funds move; a pure permission/revocation action by the maker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CancelOrderAction {
    /// Settlement venue (Seaport).
    pub venue: MarketplaceVenue,
    /// `"orders"` (cancel specific orders) | `"all"` (`incrementCounter`).
    pub scope: String,
    /// Number of specific orders cancelled. Absent when `scope = "all"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_count: Option<u32>,
}
