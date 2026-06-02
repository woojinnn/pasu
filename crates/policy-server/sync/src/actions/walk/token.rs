//! Wired: Erc20Permit.nonce, Permit2SignAllowance.nonce.

use serde_json::Value;

use policy_state::Time;
use policy_transition::action::token::{Erc20PermitAction, Permit2SignAction};
use policy_transition::action::TokenAction;

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_u256};

pub(super) fn walk(
    ta: &TokenAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match ta {
        TokenAction::Erc20Permit(p) => walk_erc20_permit(p, action_index, now, stale, stats),
        TokenAction::Permit2SignAllowance(p) => {
            walk_permit2_sign(p, action_index, now, stale, stats);
        }
        _ => {}
    }
}

fn walk_erc20_permit(
    p: &Erc20PermitAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    push_if_stale(
        stale,
        stats,
        &p.nonce,
        now,
        action_index,
        ActionSlot::TokenErc20PermitNonce,
    );
}

fn walk_permit2_sign(
    p: &Permit2SignAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    push_if_stale(
        stale,
        stats,
        &p.nonce,
        now,
        action_index,
        ActionSlot::TokenPermit2SignNonce,
    );
}

pub(super) fn apply(ta: &mut TokenAction, slot: &ActionSlot, value: Value, now: Time) {
    match (ta, slot) {
        (TokenAction::Erc20Permit(p), ActionSlot::TokenErc20PermitNonce) => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut p.nonce, u, now);
            }
        }
        (TokenAction::Permit2SignAllowance(p), ActionSlot::TokenPermit2SignNonce) => {
            if let Value::Array(arr) = &value {
                if arr.len() == 2 {
                    let word = value_to_u256(&arr[0]);
                    let bit = arr[1].as_u64().and_then(|n| u8::try_from(n).ok());
                    if let (Some(w), Some(b)) = (word, bit) {
                        set_field(&mut p.nonce, (w, b), now);
                    }
                }
            } else if let Some(u) = value_to_u256(&value) {
                set_field(&mut p.nonce, (u, 0u8), now);
            }
        }
        _ => {}
    }
}
