//! `StateDelta` — action 한 건이 만드는 변화 묶음. spec §8.
//!
//! reducer 가 만들어 반환하는 typed 변경 로그. 정책은 현재 state 와 delta 둘 다
//! 볼 수 있어야 한다 ("이 swap 이 USDC 잔고의 50% 이상을 줄이는지" 같은 정책).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// 한 pending 의 추가 / 갱신 / 제거 이벤트.
pub mod pending_change;
/// 한 포지션의 Open / Update / Close 이벤트.
pub mod position_change;
/// 한 토큰의 잔고 / approval 변경 이벤트.
pub mod token_change;

pub use pending_change::{PendingChange, PendingRemoveReason};
pub use position_change::{PositionChange, PositionPatch};
pub use token_change::{ApprovalScope, TokenChange};

use crate::primitives::U256;
use crate::token::TokenRef;

/// Action 한 건이 만드는 변화의 묶음 (token / position / pending) + 가스.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct StateDelta {
    /// 본 action 이 만든 토큰 잔고 / approval 변경 list.
    #[serde(default)]
    pub token_changes: Vec<TokenChange>,
    /// 본 action 이 만든 포지션 (Open / Update / Close) 이벤트 list.
    #[serde(default)]
    pub position_changes: Vec<PositionChange>,
    /// 본 action 이 만든 pending (서명-only) lifecycle 이벤트 list.
    #[serde(default)]
    pub pending_changes: Vec<PendingChange>,
    /// 가스 결제 (transaction 인 경우만).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "[TokenRef, string]")]
    pub gas_paid: Option<(TokenRef, U256)>,
}

impl StateDelta {
    /// 빈 `StateDelta`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 모든 변경 list 와 가스 결제가 비어 있는지.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.token_changes.is_empty()
            && self.position_changes.is_empty()
            && self.pending_changes.is_empty()
            && self.gas_paid.is_none()
    }
}
