//! PendingTx — 서명되었지만 아직 체결되지 않은 상태. spec §6.

use serde::{Deserialize, Serialize};

pub mod commitment;
pub mod kind;
pub mod nonce;

pub use commitment::AssetCommitment;
pub use kind::{OrderKind, PendingKind, PerpOrderKind};
pub use nonce::{B256, NonceKey, TxHash};

use crate::delta::StateDelta;
use crate::live_field::DataSource;
use crate::primitives::Time;

pub type PendingId = String;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    Active,
    PartiallyFilled,
    Filled,
    Cancelled,
    Expired,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingLifecycle {
    pub status: PendingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<NonceKey>,
    /// 부분 fill 또는 settler tx.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_chain_tx: Option<TxHash>,
}

impl PendingLifecycle {
    /// committed 합산에 들어가는 활성 상태인지.
    pub fn is_active_or_partial(&self) -> bool {
        matches!(
            self.status,
            PendingStatus::Active | PendingStatus::PartiallyFilled
        )
    }
}

/// 감사용 서명 페이로드. EIP-712 의 도메인 + 메시지 원본 등.
pub type SignaturePayload = Vec<u8>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTx {
    pub id: PendingId,
    pub kind: PendingKind,

    pub commitment: AssetCommitment,

    /// 체결되면 일어날 변화 (시뮬용). recursive 라서 Box.
    pub fill_effect: Box<StateDelta>,

    pub lifecycle: PendingLifecycle,

    /// pending 상태를 어디서 어떻게 갱신할지 (DataSource 와 같은 스키마).
    pub sync: DataSource,

    pub signed_at: Time,
    /// EIP-712 원본 (감사용).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature_payload: SignaturePayload,
}
