//! StateDelta — action 한 건이 만드는 변화 묶음. spec §8.
//!
//! reducer 가 만들어 반환하는 typed 변경 로그. 정책은 현재 state 와 delta 둘 다
//! 볼 수 있어야 한다 ("이 swap 이 USDC 잔고의 50% 이상을 줄이는지" 같은 정책).

use serde::{Deserialize, Serialize};

pub mod pending_change;
pub mod position_change;
pub mod token_change;

pub use pending_change::{PendingChange, PendingRemoveReason};
pub use position_change::{PositionChange, PositionPatch};
pub use token_change::{ApprovalScope, TokenChange};

use crate::primitives::U256;
use crate::token::TokenRef;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateDelta {
    #[serde(default)]
    pub token_changes: Vec<TokenChange>,
    #[serde(default)]
    pub position_changes: Vec<PositionChange>,
    #[serde(default)]
    pub pending_changes: Vec<PendingChange>,
    /// 가스 결제 (transaction 인 경우만).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gas_paid: Option<(TokenRef, U256)>,
}

impl StateDelta {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.token_changes.is_empty()
            && self.position_changes.is_empty()
            && self.pending_changes.is_empty()
            && self.gas_paid.is_none()
    }
}
