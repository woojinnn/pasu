//! PendingChange — 한 pending 의 Add/Update/Remove.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::pending::{PendingId, PendingStatus, PendingTx};
use crate::primitives::Decimal;

/// pending 이 wallet 에서 제거되는 사유.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingRemoveReason {
    /// pending 이 체결됐다.
    Filled,
    /// 사용자가 명시적으로 취소했다.
    Cancelled,
    /// deadline 이 지났다.
    Expired,
    /// 같은 nonce / orderId 로 재서명되어 대체됐다.
    Replaced,
    /// 다른 action 이 본 pending 을 무효화했다.
    SuperSeded,
}

/// 한 pending (서명-only 이벤트) 의 추가 / 갱신 / 제거 변경.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingChange {
    /// 새 pending 추가 (서명-only 이벤트). `PendingTx` 가 `StateDelta` 를 안에 들고 있어
    /// 재귀 — `Box` 로 끊음.
    Add {
        /// 새로 추가될 pending 전체.
        pending: Box<PendingTx>,
    },

    /// lifecycle 갱신.
    Update {
        /// 대상 pending 식별자.
        id: PendingId,
        /// 새 lifecycle 상태.
        status: PendingStatus,
        /// partial fill 진행 비율 (0..=1). 미해당이면 `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        partial_fill: Option<Decimal>,
    },

    /// pending 을 wallet 에서 제거.
    Remove {
        /// 제거 대상 pending 식별자.
        id: PendingId,
        /// 제거 사유.
        reason: PendingRemoveReason,
    },
}
