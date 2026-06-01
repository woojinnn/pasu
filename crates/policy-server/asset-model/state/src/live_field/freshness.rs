//! Freshness and quality metadata for a `LiveField`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::BasisPoints;

/// Confidence metadata describing how trustworthy a `LiveField` value is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Confidence {
    /// Combined uncertainty (e.g. oracle deviation, venue precision) in basis points.
    pub deviation_bp: BasisPoints,
    /// Whether the value has exceeded its TTL; populated by the Sync orchestrator.
    pub is_stale: bool,
}

impl Confidence {
    /// Returns a fully fresh `Confidence` with zero deviation and not stale.
    #[must_use]
    pub const fn fresh() -> Self {
        Self {
            deviation_bp: 0,
            is_stale: false,
        }
    }
}
