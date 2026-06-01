//! `CancelOrderAction` — cancel a previously placed open order.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::PerpVenue;

/// Cancel a previously placed open order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CancelOrderAction {
    /// Perpetual venue holding the order.
    pub venue: PerpVenue,
    /// Venue-assigned order identifier.
    pub order_id: String,
}
