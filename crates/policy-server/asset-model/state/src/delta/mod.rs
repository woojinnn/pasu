//! `StateDelta` — action 한 건이 만드는 변화 묶음. spec §8.
//!
//! reducer 가 만들어 반환하는 typed 변경 로그. 정책은 현재 state 와 delta 둘 다
//! 볼 수 있어야 한다 ("이 swap 이 USDC 잔고의 50% 이상을 줄이는지" 같은 정책).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub mod pending_change;
pub mod position_change;
pub mod token_change;

pub use pending_change::{PendingChange, PendingRemoveReason};
pub use position_change::{PositionChange, PositionPatch};
pub use token_change::{ApprovalScope, TokenChange};

use crate::primitives::U256;
use crate::token::TokenRef;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// Typed change log produced by a reducer for a single action.
///
/// Bundles all state mutations an action causes so policies can inspect both the
/// current state and the resulting delta together.
pub struct StateDelta {
    /// Token-level changes (balance deltas, approvals) caused by the action.
    #[serde(default)]
    pub token_changes: Vec<TokenChange>,
    /// Position-level changes (open / update / close) caused by the action.
    #[serde(default)]
    pub position_changes: Vec<PositionChange>,
    /// Pending-entry changes (add / update / remove) caused by the action.
    #[serde(default)]
    pub pending_changes: Vec<PendingChange>,
    /// Gas payment for the action, present only when it is a transaction
    /// (the token paid in and the amount).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "[TokenRef, string]")]
    pub gas_paid: Option<(TokenRef, U256)>,
}

impl StateDelta {
    /// Creates an empty `StateDelta` with no changes recorded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when the delta records no changes of any kind.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.token_changes.is_empty()
            && self.position_changes.is_empty()
            && self.pending_changes.is_empty()
            && self.gas_paid.is_none()
    }
}
