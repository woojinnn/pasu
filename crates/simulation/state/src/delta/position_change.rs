//! `PositionChange` — the Open/Update/Close delta applied to a single position.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsify_next::Tsify;

use crate::position::{Position, PositionId};

/// Partial-update patch carrying per-field changes as JSON (since the shape varies per change).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PositionPatch {
    /// Map of changed field path to its new value.
    /// e.g. { "`health_factor.value"`: "0.762", "collaterals[+]": [USDC, 1000] }
    #[tsify(type = "unknown")]
    pub fields: Value,
}

/// A change to a single position: opening a new one, updating an existing one, or closing one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PositionChange {
    /// A new position was opened.
    Open {
        /// The full position being opened.
        position: Position,
    },
    /// An existing position was modified.
    Update {
        /// Identifier of the position being updated.
        id: PositionId,
        /// Patch describing the changed fields and their new values.
        patch: PositionPatch,
    },
    /// An existing position was closed.
    Close {
        /// Identifier of the position being closed.
        id: PositionId,
    },
}
