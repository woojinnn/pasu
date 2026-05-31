//! `CancelLimitOrderAction` — on-chain cancel of the maker's own Pendle limit order(s).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::YieldVenue;

/// Which cancel entry on `PendleLimitRouter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum CancelKind {
    /// `cancelSingle(Order)`: cancel one order.
    Single,
    /// `cancelBatch(Order[])`: cancel a batch of orders.
    Batch,
}

/// Cancel the maker's own Pendle limit order(s) on `PendleLimitRouter`.
///
/// `cancelSingle` / `cancelBatch` invalidate orders the caller previously signed
/// as maker — self-scoped (you can only cancel your own). The pre-sign surface
/// is just "you are cancelling one / a batch of your limit orders"; the per-order
/// `Order` payload is not modeled (low-risk self-revocation, and `cancelBatch`'s
/// array length is not expressible in the `single_emit` DSL).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CancelLimitOrderAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// Single (`cancelSingle`) or batch (`cancelBatch`).
    pub kind: CancelKind,
}
