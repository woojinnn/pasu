//! `PendingTx` — signed but not-yet-settled transaction state. Spec §6.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub mod commitment;
pub mod kind;
pub mod nonce;

pub use commitment::AssetCommitment;
pub use kind::{OrderKind, PendingKind, PerpOrderKind};
pub use nonce::{NonceKey, TxHash, B256};

use crate::delta::StateDelta;
use crate::live_field::DataSource;
use crate::primitives::Time;

/// Unique identifier for a pending transaction (hash or order id, as a string).
pub type PendingId = String;

/// Lifecycle status of a signed-but-unsettled pending transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    /// Live and not yet filled; still eligible to settle.
    Active,
    /// Partially executed; remaining size is still open.
    PartiallyFilled,
    /// Fully executed.
    Filled,
    /// Cancelled before settlement.
    Cancelled,
    /// No longer valid because its validity window elapsed.
    Expired,
    /// Status could not be determined from the data source.
    Unknown,
}

/// Lifecycle tracking for a pending transaction: status, validity, and settlement linkage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingLifecycle {
    /// Current lifecycle status of the pending transaction.
    pub status: PendingStatus,
    /// Time after which the pending transaction is no longer valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub valid_until: Option<Time>,
    /// Nonce key used to detect conflicts with other pending entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub nonce: Option<NonceKey>,
    /// On-chain transaction hash of a partial fill or settler tx.
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

/// Raw signature payload kept for auditing (e.g. the EIP-712 domain + message bytes).
pub type SignaturePayload = Vec<u8>;

/// A signed but not-yet-settled transaction tracked for simulation and policy evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingTx {
    /// Unique identifier for this pending transaction.
    pub id: PendingId,
    /// The kind of pending entry (off-chain order, perp order, permit, etc.).
    pub kind: PendingKind,

    /// Assets committed (locked or potentially spendable) by this pending transaction.
    pub commitment: AssetCommitment,

    /// State change applied when this transaction settles (simulation only); boxed because it is recursive.
    pub fill_effect: Box<StateDelta>,

    /// Lifecycle status, validity, and nonce of this pending transaction.
    pub lifecycle: PendingLifecycle,

    /// Where and how to refresh the pending status (same schema as `DataSource`).
    pub sync: DataSource,

    /// Timestamp at which the transaction was signed.
    pub signed_at: Time,
    /// Original EIP-712 signature bytes, retained for auditing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[tsify(type = "Array<number>")]
    pub signature_payload: SignaturePayload,
}
