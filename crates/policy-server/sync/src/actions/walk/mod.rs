//! Action tree walker and domain dispatch for `Action.body.*.live_inputs`.
//!
//! Entry points are [`walk_action_stale`] and [`apply_value_to_action`].
//! Domain-specific walkers live in `token.rs`, `amm.rs`, `lending.rs`,
//! `airdrop.rs`, `launchpad.rs`, `perp.rs`, `permission.rs`, and
//! `liquid_staking.rs`; shared helpers in this module handle stale-field
//! collection and JSON value assignment.
use serde_json::Value;

use policy_state::{LiveField, Time};
use policy_transition::action::{Action, ActionBody};

use crate::walker::{ActionSlot, FieldLocation, StaleField, WalkStats};

pub mod airdrop;
pub mod amm;
pub mod launchpad;
pub mod lending;
pub mod liquid_staking;
pub mod permission;
pub mod perp;
pub mod token;

#[must_use]
pub fn walk_action_stale(action: &Action, now: Time) -> (Vec<StaleField>, WalkStats) {
    let mut stale = Vec::new();
    let mut stats = WalkStats::default();
    walk_body(&action.body, 0, now, &mut stale, &mut stats);
    (stale, stats)
}

fn walk_body(
    body: &ActionBody,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match body {
        ActionBody::Token(t) => token::walk(t, action_index, now, stale, stats),
        ActionBody::Amm(a) => amm::walk(a, action_index, now, stale, stats),
        ActionBody::Lending(la) => lending::walk(la, action_index, now, stale, stats),
        ActionBody::Airdrop(a) => airdrop::walk(a, action_index, now, stale, stats),
        ActionBody::Launchpad(l) => launchpad::walk(l, action_index, now, stale, stats),
        ActionBody::Perp(p) => perp::walk(p, action_index, now, stale, stats),
        ActionBody::LiquidStaking(ls) => liquid_staking::walk(ls, action_index, now, stale, stats),
        // staking actions carry no live inputs — nothing to walk.
        ActionBody::Staking(_) => {}
        // governance actions carry no live inputs — nothing to walk.
        ActionBody::Governance(_) => {}
        ActionBody::Permission(p) => permission::walk(p, action_index, now, stale, stats),
        // Yield (Pendle) carries no live_inputs in P1a — enrichment (market →
        // SY/PT/YT/maturity) is wired in P1c (the source descriptor is built at
        // decode time, no sync-side walk slot).
        ActionBody::Yield(_) => {}
        // Hyperliquid CORE actions carry NO live inputs (they are self-describing
        // order/transfer intents), so there is nothing to refresh — like Unknown.
        ActionBody::HyperliquidCore(_) => {}
        // bridge actions carry no live inputs — nothing to walk.
        ActionBody::Bridge(_) => {}
        // Marketplace (Seaport) actions carry no live inputs — nothing to walk.
        ActionBody::Marketplace(_) => {}
        ActionBody::Multicall { actions } => {
            for (i, child) in actions.iter().enumerate() {
                walk_body(child, i, now, stale, stats);
            }
        }
        // Restaking round-1 actions carry no live inputs (no walk needed).
        ActionBody::Restaking(_) => {}
        ActionBody::Unknown { .. } => {}
    }
}

pub fn apply_value_to_action(
    action: &mut Action,
    location: &FieldLocation,
    value: Value,
    now: Time,
) {
    let FieldLocation::Action { action_index, slot } = location else {
        return;
    };

    let body = body_at_index_mut(&mut action.body, *action_index);
    let Some(body) = body else { return };

    match body {
        ActionBody::Token(t) => token::apply(t, slot, value, now),
        ActionBody::Amm(a) => amm::apply(a, slot, value, now),
        ActionBody::Lending(la) => lending::apply(la, slot, value, now),
        ActionBody::Airdrop(a) => airdrop::apply(a, slot, value, now),
        ActionBody::Launchpad(l) => launchpad::apply(l, slot, value, now),
        ActionBody::Perp(p) => perp::apply(p, slot, value, now),
        ActionBody::Permission(p) => permission::apply(p, slot, value, now),
        ActionBody::LiquidStaking(ls) => liquid_staking::apply(ls, slot, value, now),
        // No live_input apply-slots on Yield / Restaking / Staking / Hyperliquid CORE
        // → nothing to apply (Yield P1c enrichment is built at decode time, not synced).
        ActionBody::Yield(_)
        | ActionBody::Restaking(_)
        | ActionBody::Staking(_)
        | ActionBody::Governance(_)
        | ActionBody::HyperliquidCore(_)
        | ActionBody::Bridge(_)
        | ActionBody::Marketplace(_)
        | ActionBody::Multicall { .. }
        | ActionBody::Unknown { .. } => {}
    }
}

pub(crate) fn body_at_index_mut(body: &mut ActionBody, index: usize) -> Option<&mut ActionBody> {
    match body {
        ActionBody::Multicall { actions } => actions.get_mut(index),
        single if index == 0 => Some(single),
        _ => None,
    }
}

pub(crate) fn push_if_stale<T>(
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
    field: &LiveField<T>,
    now: Time,
    action_index: usize,
    slot: ActionSlot,
) {
    stats.total_live_fields += 1;
    if field.is_stale(now) {
        stats.stale_count += 1;
        stale.push(StaleField {
            location: FieldLocation::Action { action_index, slot },
            source: field.source.clone(),
            synced_at: field.synced_at,
        });
    } else {
        stats.fresh_count += 1;
    }
}

pub(crate) fn set_field<T>(field: &mut LiveField<T>, value: T, now: Time) {
    field.value = value;
    field.synced_at = now;
    field.confidence = Some(policy_state::Confidence::fresh());
}

pub(crate) fn value_to_decimal(v: &Value) -> Option<policy_state::Decimal> {
    match v {
        Value::String(s) => Some(policy_state::Decimal::new(s.clone())),
        Value::Number(n) => Some(policy_state::Decimal::new(n.to_string())),
        _ => None,
    }
}

pub(crate) fn value_to_u256(v: &Value) -> Option<policy_state::U256> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    policy_state::U256::from_str_radix(&s, 10).ok()
}
