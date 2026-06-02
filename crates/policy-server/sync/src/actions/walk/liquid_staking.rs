//! Liquid-staking action walker and updater.
//!
//! Wired: `Wrap` (`expected_wsteth`), `Unwrap` (`expected_steth`),
//!        `TransferShares` (`pooled_eth`), each as a single `uint256` view.
//! `Stake` / `RequestWithdrawal` / `ClaimWithdrawal` have no `live_inputs` → no-op.

use serde_json::Value;

use policy_state::Time;
use policy_transition::action::liquid_staking::{
    LiquidStakingAction, TransferSharesAction, UnwrapAction, WrapAction,
};

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_u256};

// ─────────────────────── walk ───────────────────────

pub(super) fn walk(
    ls: &LiquidStakingAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match ls {
        LiquidStakingAction::Wrap(w) => walk_wrap(w, action_index, now, stale, stats),
        LiquidStakingAction::Unwrap(u) => walk_unwrap(u, action_index, now, stale, stats),
        LiquidStakingAction::TransferShares(t) => {
            walk_transfer_shares(t, action_index, now, stale, stats);
        }
        LiquidStakingAction::Stake(_) => {} // no live_inputs
        LiquidStakingAction::RequestWithdrawal(_) => {} // no live_inputs
        LiquidStakingAction::ClaimWithdrawal(_) => {} // no live_inputs
    }
}

fn walk_wrap(w: &WrapAction, ix: usize, now: Time, st: &mut Vec<StaleField>, sx: &mut WalkStats) {
    push_if_stale(
        st,
        sx,
        &w.live_inputs.expected_wsteth,
        now,
        ix,
        ActionSlot::LiquidStakingWrapExpectedWsteth,
    );
}

fn walk_unwrap(
    u: &UnwrapAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    push_if_stale(
        st,
        sx,
        &u.live_inputs.expected_steth,
        now,
        ix,
        ActionSlot::LiquidStakingUnwrapExpectedSteth,
    );
}

fn walk_transfer_shares(
    t: &TransferSharesAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    push_if_stale(
        st,
        sx,
        &t.live_inputs.pooled_eth,
        now,
        ix,
        ActionSlot::LiquidStakingTransferSharesPooledEth,
    );
}

// ─────────────────────── apply ───────────────────────

pub(super) fn apply(ls: &mut LiquidStakingAction, slot: &ActionSlot, value: Value, now: Time) {
    match ls {
        LiquidStakingAction::Wrap(w) => apply_wrap(w, slot, value, now),
        LiquidStakingAction::Unwrap(u) => apply_unwrap(u, slot, value, now),
        LiquidStakingAction::TransferShares(t) => apply_transfer_shares(t, slot, value, now),
        LiquidStakingAction::Stake(_)
        | LiquidStakingAction::RequestWithdrawal(_)
        | LiquidStakingAction::ClaimWithdrawal(_) => {}
    }
}

fn apply_wrap(w: &mut WrapAction, slot: &ActionSlot, value: Value, now: Time) {
    if matches!(slot, ActionSlot::LiquidStakingWrapExpectedWsteth) {
        if let Some(v) = value_to_u256(&value) {
            set_field(&mut w.live_inputs.expected_wsteth, v, now);
        }
    }
}

fn apply_unwrap(u: &mut UnwrapAction, slot: &ActionSlot, value: Value, now: Time) {
    if matches!(slot, ActionSlot::LiquidStakingUnwrapExpectedSteth) {
        if let Some(v) = value_to_u256(&value) {
            set_field(&mut u.live_inputs.expected_steth, v, now);
        }
    }
}

fn apply_transfer_shares(t: &mut TransferSharesAction, slot: &ActionSlot, value: Value, now: Time) {
    if matches!(slot, ActionSlot::LiquidStakingTransferSharesPooledEth) {
        if let Some(v) = value_to_u256(&value) {
            set_field(&mut t.live_inputs.pooled_eth, v, now);
        }
    }
}
