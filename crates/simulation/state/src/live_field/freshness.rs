//! `LiveField` 의 신선도/품질 메타.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::BasisPoints;

/// `LiveField` 의 신선도 / 품질 메타.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Confidence {
    /// oracle deviation, venue precision 등. basis points.
    pub deviation_bp: BasisPoints,
    /// ttl 초과 여부 — Sync orchestrator 가 채움.
    pub is_stale: bool,
}

impl Confidence {
    /// deviation 0 + 신선 (not stale) 한 `Confidence`.
    #[must_use]
    pub fn fresh() -> Self {
        Self {
            deviation_bp: 0,
            is_stale: false,
        }
    }
}
