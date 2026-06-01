//! Wired: Claim (4 slots), Delegate (2 slots).

use serde_json::Value;

use policy_state::Time;
use policy_transition::action::airdrop::{ClaimAirdropAction, DelegateGovernanceAction};
use policy_transition::action::AirdropAction;

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_u256};

pub(super) fn walk(
    aa: &AirdropAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match aa {
        AirdropAction::Claim(c) => walk_claim(c, action_index, now, stale, stats),
        AirdropAction::Delegate(d) => walk_delegate(d, action_index, now, stale, stats),
    }
}

fn walk_claim(
    c: &ClaimAirdropAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.is_still_claimable,
        now,
        action_index,
        ActionSlot::AirdropClaimIsStillClaimable,
    );
    push_if_stale(
        stale,
        stats,
        &li.actual_amount,
        now,
        action_index,
        ActionSlot::AirdropClaimActualAmount,
    );
    push_if_stale(
        stale,
        stats,
        &li.claim_token,
        now,
        action_index,
        ActionSlot::AirdropClaimToken,
    );
    push_if_stale(
        stale,
        stats,
        &li.claim_window,
        now,
        action_index,
        ActionSlot::AirdropClaimWindow,
    );
}

fn walk_delegate(
    d: &DelegateGovernanceAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &d.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.current_delegate,
        now,
        action_index,
        ActionSlot::AirdropDelegateCurrentDelegate,
    );
    push_if_stale(
        stale,
        stats,
        &li.voting_power,
        now,
        action_index,
        ActionSlot::AirdropDelegateVotingPower,
    );
}

pub(super) fn apply(aa: &mut AirdropAction, slot: &ActionSlot, value: Value, now: Time) {
    match (aa, slot) {
        (AirdropAction::Claim(c), s) => apply_claim(c, s, value, now),
        (AirdropAction::Delegate(d), s) => apply_delegate(d, s, value, now),
    }
}

fn apply_claim(c: &mut ClaimAirdropAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::AirdropClaimIsStillClaimable => {
            if let Value::Bool(b) = value {
                set_field(&mut li.is_still_claimable, b, now);
            }
        }
        ActionSlot::AirdropClaimActualAmount => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.actual_amount, u, now);
            }
        }
        ActionSlot::AirdropClaimToken => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.claim_token, v, now);
            }
        }
        ActionSlot::AirdropClaimWindow => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.claim_window, v, now);
            }
        }
        _ => {}
    }
}

fn apply_delegate(d: &mut DelegateGovernanceAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut d.live_inputs;
    match slot {
        ActionSlot::AirdropDelegateCurrentDelegate => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.current_delegate, v, now);
            }
        }
        ActionSlot::AirdropDelegateVotingPower => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.voting_power, u, now);
            }
        }
        _ => {}
    }
}
