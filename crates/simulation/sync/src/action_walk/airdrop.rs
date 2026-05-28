//! Airdrop 도메인 walk + apply.
//!
//! 현재 wired: (아직 없음). 후속에서 채움.

use serde_json::Value;

use simulation_reducer::action::AirdropAction;
use simulation_state::Time;

use crate::walker::{ActionSlot, StaleField, WalkStats};

pub(super) fn walk(
    _aa: &AirdropAction,
    _action_index: usize,
    _now: Time,
    _stale: &mut Vec<StaleField>,
    _stats: &mut WalkStats,
) {
    // TODO: ClaimAirdropAction.live_inputs (is_still_claimable, actual_amount,
    //       claim_token, claim_window), DelegateGovernanceAction.live_inputs
    //       (current_delegate, voting_power)
}

pub(super) fn apply(_aa: &mut AirdropAction, _slot: &ActionSlot, _value: Value, _now: Time) {
    // TODO
}
