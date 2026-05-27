//! LiveField 의 신선도/품질 메타.

use serde::{Deserialize, Serialize};

use crate::primitives::BasisPoints;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confidence {
    /// oracle deviation, venue precision 등. basis points.
    pub deviation_bp: BasisPoints,
    /// ttl 초과 여부 — Sync orchestrator 가 채움.
    pub is_stale: bool,
}

impl Confidence {
    pub fn fresh() -> Self {
        Self {
            deviation_bp: 0,
            is_stale: false,
        }
    }
}
