//! Launchpad 도메인 walk + apply.
//!
//! 현재 wired: (아직 없음). 후속에서 채움.

use serde_json::Value;

use simulation_reducer::action::LaunchpadAction;
use simulation_state::Time;

use crate::walker::{ActionSlot, StaleField, WalkStats};

pub(super) fn walk(
    _la: &LaunchpadAction,
    _action_index: usize,
    _now: Time,
    _stale: &mut Vec<StaleField>,
    _stats: &mut WalkStats,
) {
    // TODO: CommitAction (sale_state/user_cap/user_committed/expected_token_price),
    //       ClaimAllocation, ClaimVested, Refund, WithdrawCommit
}

pub(super) fn apply(_la: &mut LaunchpadAction, _slot: &ActionSlot, _value: Value, _now: Time) {
    // TODO
}
