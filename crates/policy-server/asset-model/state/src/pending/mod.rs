//! `PendingTx` represents a signed request that has not fully settled yet.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// Asset commitment model for pending entries.
pub mod commitment;
/// Pending entry kinds.
pub mod kind;
/// Pending nonce and hash identifiers.
pub mod nonce;

pub use commitment::AssetCommitment;
pub use kind::{OrderKind, PendingKind, PerpOrderKind};
pub use nonce::{NonceKey, TxHash, B256};

use crate::delta::StateDelta;
use crate::live_field::DataSource;
use crate::primitives::Time;

/// Stable identifier for a pending entry.
pub type PendingId = String;

/// Lifecycle status for a pending entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    /// Signed and not yet settled.
    Active,
    /// Partially settled.
    PartiallyFilled,
    /// Fully settled.
    Filled,
    /// Cancelled by the user.
    Cancelled,
    /// Expired after its deadline.
    Expired,
    /// Unknown because the venue did not respond or reconciliation failed.
    Unknown,
}

/// Lifecycle metadata for a pending entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingLifecycle {
    /// Current lifecycle status.
    pub status: PendingStatus,
    /// Deadline while this pending entry remains valid; `None` means no deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub valid_until: Option<Time>,
    /// Nonce or order hash for this pending entry; `None` when unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub nonce: Option<NonceKey>,
    /// Partial-fill or settlement transaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub on_chain_tx: Option<TxHash>,
}

impl PendingLifecycle {
    /// Whether the status counts toward committed totals (active or partially filled).
    #[must_use]
    pub const fn is_active_or_partial(&self) -> bool {
        matches!(
            self.status,
            PendingStatus::Active | PendingStatus::PartiallyFilled
        )
    }
}

/// Signature payload retained for audit, such as the original EIP-712 domain and message.
pub type SignaturePayload = Vec<u8>;

/// Body for a signature-only or unsettled pending entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingTx {
    /// Pending entry identifier.
    pub id: PendingId,
    /// Pending sub-kind and payload.
    pub kind: PendingKind,

    /// How assets are committed while this entry is pending.
    pub commitment: AssetCommitment,

    /// Simulated state change that would happen on fill; boxed because deltas are recursive.
    pub fill_effect: Box<StateDelta>,

    /// Lifecycle metadata.
    pub lifecycle: PendingLifecycle,

    /// Source used to refresh this pending entry's status.
    pub sync: DataSource,

    /// Signature timestamp.
    pub signed_at: Time,
    /// Original EIP-712 payload bytes retained for audit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[tsify(type = "Array<number>")]
    pub signature_payload: SignaturePayload,
}
