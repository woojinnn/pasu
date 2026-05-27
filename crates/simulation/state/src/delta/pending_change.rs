//! PendingChange — 한 pending 의 Add/Update/Remove.

use serde::{Deserialize, Serialize};

use crate::pending::{PendingId, PendingStatus, PendingTx};
use crate::primitives::Decimal;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingRemoveReason {
    Filled,
    Cancelled,
    Expired,
    Replaced,
    SuperSeded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingChange {
    /// 새 pending 추가 (서명-only 이벤트). PendingTx 가 StateDelta 를 안에 들고 있어
    /// 재귀 — Box 로 끊음.
    Add { pending: Box<PendingTx> },

    /// lifecycle 갱신.
    Update {
        id: PendingId,
        status: PendingStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        partial_fill: Option<Decimal>,
    },

    Remove {
        id: PendingId,
        reason: PendingRemoveReason,
    },
}
