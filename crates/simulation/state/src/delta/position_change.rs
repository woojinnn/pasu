//! PositionChange — 한 포지션에 대한 Open/Update/Close.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::position::{Position, PositionId};

/// 부분 갱신 패치. 필드별 변화를 JSON 으로 들고 다닌다 (variant 마다 다르므로).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionPatch {
    /// 변경된 필드 path → 새 값.
    /// 예: { "health_factor.value": "0.762", "collaterals[+]": [USDC, 1000] }
    pub fields: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PositionChange {
    Open { position: Position },
    Update { id: PositionId, patch: PositionPatch },
    Close { id: PositionId },
}
