//! `PendingTx` — 서명되었지만 아직 체결되지 않은 상태. spec §6.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// pending 의 자산 묶임 방식 (`AssetCommitment`).
pub mod commitment;
/// pending 의 4가지 종류 (`PendingKind`).
pub mod kind;
/// pending nonce / hash 식별자 (`NonceKey`, `B256`, `TxHash`).
pub mod nonce;

pub use commitment::AssetCommitment;
pub use kind::{OrderKind, PendingKind, PerpOrderKind};
pub use nonce::{NonceKey, TxHash, B256};

use crate::delta::StateDelta;
use crate::live_field::DataSource;
use crate::primitives::Time;

/// 한 pending 의 안정 식별자 (string).
pub type PendingId = String;

/// 한 pending 의 lifecycle 상태.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    /// 서명 완료, 미체결.
    Active,
    /// 일부 체결됨.
    PartiallyFilled,
    /// 완전 체결.
    Filled,
    /// 사용자 취소.
    Cancelled,
    /// deadline 만료.
    Expired,
    /// venue 응답 부재 / 갱신 실패 등.
    Unknown,
}

/// pending 의 lifecycle 메타 (status / `valid_until` / nonce / on-chain tx).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingLifecycle {
    /// 현재 lifecycle 상태.
    pub status: PendingStatus,
    /// 본 pending 이 유효한 deadline. 무기한이면 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub valid_until: Option<Time>,
    /// 본 pending 의 nonce / order hash. nonce 가 없는 형태는 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub nonce: Option<NonceKey>,
    /// 부분 fill 또는 settler tx.
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

/// 감사용 서명 페이로드. EIP-712 의 도메인 + 메시지 원본 등.
pub type SignaturePayload = Vec<u8>;

/// 서명-only / 미체결 pending entry 본체.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingTx {
    /// 본 pending 의 식별자.
    pub id: PendingId,
    /// pending 의 sub-kind 와 본체 데이터.
    pub kind: PendingKind,

    /// 자산이 어떻게 묶여 있는지.
    pub commitment: AssetCommitment,

    /// 체결되면 일어날 변화 (시뮬용). recursive 라서 Box.
    pub fill_effect: Box<StateDelta>,

    /// lifecycle 메타.
    pub lifecycle: PendingLifecycle,

    /// pending 상태를 어디서 어떻게 갱신할지 (`DataSource` 와 같은 스키마).
    pub sync: DataSource,

    /// 서명 시각.
    pub signed_at: Time,
    /// EIP-712 원본 (감사용).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[tsify(type = "Array<number>")]
    pub signature_payload: SignaturePayload,
}
