//! Simulator-based envelope merge.
//!
//! Replaces the pattern-matching collapse in `super::merge` with a small
//! ledger that watches asset flow across an envelope sequence. After all
//! envelopes have been applied to the ledger, the user-side net delta is
//! interpreted: a `(1 spent, 1 received)` shape collapses into a single
//! `SwapAction`, anything else falls back to the original envelope list.
//!
//! Module layout:
//!   - `effect`     — Effect / ActorRef / Asset / AmountSpec vocabulary
//!   - `ledger`     — Ledger / Bucket / Constraint, applies effects
//!   - `effects_of` — envelope → Effect[] (per-variant)

#![allow(dead_code)] // PR 10a — wired up in PR 10b. Allow unused while infra lands.

mod effect;
mod effects_of;
mod ledger;

use std::str::FromStr as _;

use alloy_primitives::{I256, U256};
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{
    Action, ActionEnvelope, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity,
};

use crate::CallContext;

use effect::{ActorRef, AmountSpec, Asset, Effect};
use effects_of::{asset_from_ref, effects_of};
use ledger::{Bucket, Constraint, Ledger};

/// Top-level entry point. Apply every envelope to a fresh ledger and let
/// the interpreter decide whether the deltas describe a single semantic
/// action (collapsed envelope) or fall back to the per-opcode fan-out.
pub(in crate::multi_router) fn simulate(
    envelopes: Vec<ActionEnvelope>,
    ctx: &CallContext<'_>,
) -> Vec<ActionEnvelope> {
    let mut ledger = Ledger::new();

    // Initial state: msg.value moves from User to Router. EVM applies this
    // before any calldata runs; the simulator mirrors it so wrap/swap
    // accounting starts from the correct base.
    if let Ok(value) = U256::from_str(&ctx.value_wei.to_string()) {
        if value > U256::ZERO {
            ledger.apply(
                Effect::Move {
                    from: ActorRef::User,
                    to: ActorRef::Router,
                    asset: Asset::Native,
                    amount: AmountSpec::Exact(value),
                },
                ctx,
            );
        }
    }

    for env in &envelopes {
        for effect in effects_of(env) {
            ledger.apply(effect, ctx);
        }
    }

    interpret(&ledger, &envelopes, ctx)
}

/// Convert the ledger's user-side net delta into the smallest envelope
/// list that faithfully represents it. Unsupported shapes fall back to
/// the original `envelopes` list (lossy fan-out is preferable to a
/// wrong merged envelope).
fn interpret(
    ledger: &Ledger,
    fallback: &[ActionEnvelope],
    ctx: &CallContext<'_>,
) -> Vec<ActionEnvelope> {
    let delta = ledger.user_delta(ctx.from);

    // Unknown constraints in any user-visible bucket → can't soundly merge.
    if delta.iter().any(|(_, b)| matches!(b.constraint, Constraint::Unknown)) {
        return fallback.to_vec();
    }

    let (spent, received): (Vec<_>, Vec<_>) = delta
        .into_iter()
        .partition(|(_, b)| b.net.is_negative());

    match (spent.len(), received.len()) {
        (1, 1) => {
            // Pull a representative SwapAction from the fallback envelopes
            // to inherit metadata we don't reconstruct (validity, fee_bps,
            // mode, enrichment). If none exists, skip the merge — the
            // simulator can describe the asset flow but shouldn't invent
            // a swap from nothing.
            let template = fallback.iter().find_map(|e| match &e.action {
                Action::Swap(s) => Some(s.clone()),
                _ => None,
            });
            let Some(template) = template else {
                return fallback.to_vec();
            };
            let merged = build_swap(spent[0].clone(), received[0].clone(), template, ctx);
            vec![ActionEnvelope {
                category: Category::Dex,
                action: Action::Swap(merged),
            }]
        }
        _ => fallback.to_vec(),
    }
}

fn build_swap(
    spent: (Asset, Bucket),
    received: (Asset, Bucket),
    template: SwapAction,
    ctx: &CallContext<'_>,
) -> SwapAction {
    let (spent_asset, spent_bucket) = spent;
    let (received_asset, received_bucket) = received;

    let amount_in = bucket_to_constraint(spent_bucket, /* is_input */ true);
    let amount_out = bucket_to_constraint(received_bucket, /* is_input */ false);

    SwapAction {
        // Mode follows whichever side is exact (typical for V2/V3 swaps).
        // Falls back to the template's mode if the deltas are ambiguous.
        mode: derive_mode(spent_bucket.constraint, received_bucket.constraint, template.mode),
        token_in: asset_to_ref(spent_asset, ctx.chain_id),
        token_out: asset_to_ref(received_asset, ctx.chain_id),
        amount_in,
        amount_out,
        recipient: ctx.from.clone(),
        validity: template.validity,
        fee_bps: template.fee_bps,
        enrichment: template.enrichment,
    }
}

fn bucket_to_constraint(bucket: Bucket, is_input: bool) -> AmountConstraint {
    let abs = bucket.net.unsigned_abs();
    let value = DecimalString::from_str(&abs.to_string()).ok();
    let kind = match (bucket.constraint, is_input) {
        (Constraint::Exact, _) => AmountKind::Exact,
        (Constraint::AtLeast, true) => AmountKind::Max,   // user spent at least X → cap is X
        (Constraint::AtLeast, false) => AmountKind::Min,
        (Constraint::AtMost, true) => AmountKind::Max,
        (Constraint::AtMost, false) => AmountKind::Min,
        // Unknown should be filtered before this; treat as Exact defensively.
        (Constraint::Unknown, _) => AmountKind::Exact,
    };
    AmountConstraint { kind, value }
}

fn derive_mode(input: Constraint, output: Constraint, fallback: SwapMode) -> SwapMode {
    match (input, output) {
        (Constraint::Exact, _) => SwapMode::ExactIn,
        (_, Constraint::Exact) => SwapMode::ExactOut,
        _ => fallback,
    }
}

fn asset_to_ref(a: Asset, chain_id: u64) -> AssetRef {
    match a {
        Asset::Native => AssetRef {
            kind: AssetKind::Native,
            chain_id,
            address: None,
            symbol: Some("ETH".to_owned()),
            decimals: Some(18),
        },
        Asset::Erc20(addr) => AssetRef {
            kind: AssetKind::Erc20,
            chain_id,
            address: Some(addr),
            symbol: None,
            decimals: None,
        },
    }
}

// I256.unsigned_abs() helper — alloy's I256 doesn't expose one directly,
// so do it via str round-trip (small simulator amounts only).
trait I256AbsExt {
    fn unsigned_abs(&self) -> U256;
}

impl I256AbsExt for I256 {
    fn unsigned_abs(&self) -> U256 {
        if self.is_negative() {
            // -x where x is the magnitude
            U256::from_str(&self.to_string().trim_start_matches('-')).unwrap_or(U256::ZERO)
        } else {
            U256::from_str(&self.to_string()).unwrap_or(U256::ZERO)
        }
    }
}
