//! Launchpad 도메인 walk + apply.
//!
//! Wired: Commit (4), `ClaimAllocation` (3), `ClaimVested` (2), Refund (2),
//!        `WithdrawCommit` (2). 총 13 slots.

use serde_json::Value;

use simulation_reducer::action::launchpad::{
    ClaimAllocationAction, ClaimVestedAction, CommitAction, RefundAction, WithdrawCommitAction,
};
use simulation_reducer::action::LaunchpadAction;
use simulation_state::Time;

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_u256};

pub(super) fn walk(
    la: &LaunchpadAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match la {
        LaunchpadAction::Commit(c) => walk_commit(c, action_index, now, stale, stats),
        LaunchpadAction::ClaimAllocation(c) => walk_claim_alloc(c, action_index, now, stale, stats),
        LaunchpadAction::ClaimVested(c) => walk_claim_vested(c, action_index, now, stale, stats),
        LaunchpadAction::Refund(r) => walk_refund(r, action_index, now, stale, stats),
        LaunchpadAction::WithdrawCommit(w) => walk_withdraw(w, action_index, now, stale, stats),
    }
}

fn walk_commit(
    c: &CommitAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.sale_state,
        now,
        action_index,
        ActionSlot::LaunchpadCommitSaleState,
    );
    push_if_stale(
        stale,
        stats,
        &li.user_cap,
        now,
        action_index,
        ActionSlot::LaunchpadCommitUserCap,
    );
    push_if_stale(
        stale,
        stats,
        &li.user_committed,
        now,
        action_index,
        ActionSlot::LaunchpadCommitUserCommitted,
    );
    push_if_stale(
        stale,
        stats,
        &li.expected_token_price,
        now,
        action_index,
        ActionSlot::LaunchpadCommitExpectedTokenPrice,
    );
}

fn walk_claim_alloc(
    c: &ClaimAllocationAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.allocated,
        now,
        action_index,
        ActionSlot::LaunchpadClaimAllocationAllocated,
    );
    push_if_stale(
        stale,
        stats,
        &li.refund_due,
        now,
        action_index,
        ActionSlot::LaunchpadClaimAllocationRefundDue,
    );
    push_if_stale(
        stale,
        stats,
        &li.is_claimable,
        now,
        action_index,
        ActionSlot::LaunchpadClaimAllocationIsClaimable,
    );
}

fn walk_claim_vested(
    c: &ClaimVestedAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.claimable_now,
        now,
        action_index,
        ActionSlot::LaunchpadClaimVestedClaimableNow,
    );
    push_if_stale(
        stale,
        stats,
        &li.next_unlock,
        now,
        action_index,
        ActionSlot::LaunchpadClaimVestedNextUnlock,
    );
}

fn walk_refund(
    r: &RefundAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &r.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.refund_amount,
        now,
        action_index,
        ActionSlot::LaunchpadRefundAmount,
    );
    push_if_stale(
        stale,
        stats,
        &li.refund_token,
        now,
        action_index,
        ActionSlot::LaunchpadRefundToken,
    );
}

fn walk_withdraw(
    w: &WithdrawCommitAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &w.live_inputs;
    push_if_stale(
        stale,
        stats,
        &li.withdrawable,
        now,
        action_index,
        ActionSlot::LaunchpadWithdrawCommitWithdrawable,
    );
    push_if_stale(
        stale,
        stats,
        &li.sale_state,
        now,
        action_index,
        ActionSlot::LaunchpadWithdrawCommitSaleState,
    );
}

pub(super) fn apply(la: &mut LaunchpadAction, slot: &ActionSlot, value: Value, now: Time) {
    match la {
        LaunchpadAction::Commit(c) => apply_commit(c, slot, value, now),
        LaunchpadAction::ClaimAllocation(c) => apply_claim_alloc(c, slot, value, now),
        LaunchpadAction::ClaimVested(c) => apply_claim_vested(c, slot, value, now),
        LaunchpadAction::Refund(r) => apply_refund(r, slot, value, now),
        LaunchpadAction::WithdrawCommit(w) => apply_withdraw(w, slot, value, now),
    }
}

fn apply_commit(c: &mut CommitAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::LaunchpadCommitSaleState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.sale_state, v, now);
            }
        }
        ActionSlot::LaunchpadCommitUserCap => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.user_cap, u, now);
            }
        }
        ActionSlot::LaunchpadCommitUserCommitted => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.user_committed, u, now);
            }
        }
        ActionSlot::LaunchpadCommitExpectedTokenPrice => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.expected_token_price, v, now);
            }
        }
        _ => {}
    }
}

fn apply_claim_alloc(c: &mut ClaimAllocationAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::LaunchpadClaimAllocationAllocated => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.allocated, v, now);
            }
        }
        ActionSlot::LaunchpadClaimAllocationRefundDue => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.refund_due, u, now);
            }
        }
        ActionSlot::LaunchpadClaimAllocationIsClaimable => {
            if let Value::Bool(b) = value {
                set_field(&mut li.is_claimable, b, now);
            }
        }
        _ => {}
    }
}

fn apply_claim_vested(c: &mut ClaimVestedAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::LaunchpadClaimVestedClaimableNow => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.claimable_now, u, now);
            }
        }
        ActionSlot::LaunchpadClaimVestedNextUnlock => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.next_unlock, v, now);
            }
        }
        _ => {}
    }
}

fn apply_refund(r: &mut RefundAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut r.live_inputs;
    match slot {
        ActionSlot::LaunchpadRefundAmount => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.refund_amount, u, now);
            }
        }
        ActionSlot::LaunchpadRefundToken => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.refund_token, v, now);
            }
        }
        _ => {}
    }
}

fn apply_withdraw(w: &mut WithdrawCommitAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut w.live_inputs;
    match slot {
        ActionSlot::LaunchpadWithdrawCommitWithdrawable => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.withdrawable, u, now);
            }
        }
        ActionSlot::LaunchpadWithdrawCommitSaleState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.sale_state, v, now);
            }
        }
        _ => {}
    }
}
