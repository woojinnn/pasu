//! `PendingChange` — the Add/Update/Remove operations applied to a single pending tx.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::pending::{PendingId, PendingStatus, PendingTx};
use crate::primitives::Decimal;

/// Reason a pending tx is being removed from the pending set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingRemoveReason {
    /// The pending tx was fully filled / executed on-chain.
    Filled,
    /// The pending tx was cancelled by the user.
    Cancelled,
    /// The pending tx passed its validity window without being filled.
    Expired,
    /// The pending tx was replaced by a newer tx (e.g. nonce reuse).
    Replaced,
    /// The pending tx was superseded by another change and is no longer relevant.
    SuperSeded,
}

/// A single mutation to the pending set: add, update, or remove a pending tx.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingChange {
    /// Add a new pending tx (a signed-only event). `PendingTx` transitively holds a
    /// `StateDelta`, so the payload is boxed to break the recursive type.
    Add {
        /// The pending tx being added to the pending set.
        pending: Box<PendingTx>,
    },

    /// Update the lifecycle of an existing pending tx.
    Update {
        /// Identifier of the pending tx to update.
        id: PendingId,
        /// New lifecycle status for the pending tx.
        status: PendingStatus,
        /// Amount filled so far, present when the tx is partially filled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        partial_fill: Option<Decimal>,
    },

    /// Remove an existing pending tx from the pending set.
    Remove {
        /// Identifier of the pending tx to remove.
        id: PendingId,
        /// Reason the pending tx is being removed.
        reason: PendingRemoveReason,
    },
}
