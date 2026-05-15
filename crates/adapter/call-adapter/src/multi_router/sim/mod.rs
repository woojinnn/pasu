//! Asset-flow simulator for collapsing UR-style envelope sequences.
//!
//! Replaces the pattern-matching merge in `super::merge`. Walks the
//! envelopes in order, applies asset effects to a virtual ledger, then
//! reads the user's net delta and rebuilds a single semantic envelope
//! when possible.
//!
//! Why ledger-aware (not pure `effects_of(env)`):
//!
//! Universal Router patterns split asset ownership across opcodes —
//! WRAP_ETH leaves WETH on the router, then V3_SWAP consumes from the
//! router (router is the swap payer); the inverse for SWAP→UNWRAP. A
//! pure per-envelope effect translator would have to guess the swap
//! payer (User vs Router) without context. Instead `simulate` decides
//! the payer at apply-time by querying the ledger: if the router
//! already holds the swap input, the router pays; otherwise the user
//! does (the standard transferFrom path).
//!
//! Module layout:
//!   - `effect`     — Effect / ActorRef / Asset / AmountSpec vocabulary
//!   - `ledger`     — Ledger / Bucket / Constraint, applies effects
//!   - `effects_of` — small helpers (asset_from_ref, amount_spec_from)

mod effect;
mod effects_of;
mod ledger;

#[cfg(test)]
mod tests;

use std::str::FromStr as _;

use alloy_primitives::{I256, U256};
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::misc::{UnwrapAction, WrapAction};
use policy_engine::action::{
    Action, ActionEnvelope, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString,
};

use crate::CallContext;

use effect::{ActorRef, AmountSpec, Asset, Effect};
use effects_of::{amount_spec_from, asset_from_ref};
use ledger::{Bucket, Constraint, Ledger};

/// Top-level entry point. Apply every envelope to a fresh ledger and let
/// the interpreter decide whether the deltas describe a single semantic
/// action (collapsed envelope) or fall back to the per-opcode fan-out.
pub(super) fn simulate(
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
        apply_envelope(&mut ledger, env, ctx);
    }

    interpret(&ledger, &envelopes, ctx)
}

/// Dispatch one envelope to the ledger, deciding actor refs from current
/// ledger state where the envelope itself doesn't pin them down.
fn apply_envelope(ledger: &mut Ledger, env: &ActionEnvelope, ctx: &CallContext<'_>) {
    match &env.action {
        Action::Swap(s) => apply_swap(ledger, s, ctx),
        Action::Wrap(w) => apply_wrap(ledger, w, ctx),
        Action::Unwrap(u) => apply_unwrap(ledger, u, ctx),
        // Other variants (Approve, Permit, lending sign actions, …) don't
        // move asset balances at this layer.
        _ => {}
    }
}

/// SwapAction → Burn(payer) + Mint(recipient).
///
/// Payer choice is the only ambiguous bit: the envelope schema doesn't
/// carry it, and on Universal Router the router is the payer when a
/// preceding WRAP_ETH (or PERMIT2_TRANSFER_FROM, or prior swap output)
/// staged the input on the router. We resolve at apply-time by reading
/// the ledger — if the router already holds the input asset, it pays;
/// otherwise the user does (covering the simple direct-from-user case).
fn apply_swap(ledger: &mut Ledger, s: &SwapAction, ctx: &CallContext<'_>) {
    let token_in = asset_from_ref(&s.token_in);
    let token_out = asset_from_ref(&s.token_out);

    let payer = decide_swap_payer(ledger, &token_in, ctx);
    if let Some(amount) = amount_spec_from(&s.amount_in) {
        ledger.apply(
            Effect::Burn {
                from: payer,
                asset: token_in,
                amount,
            },
            ctx,
        );
    }
    if let Some(amount) = amount_spec_from(&s.amount_out) {
        ledger.apply(
            Effect::Mint {
                to: ActorRef::External(s.recipient.clone()),
                asset: token_out,
                amount,
            },
            ctx,
        );
    }
}

/// WrapAction (UR `WRAP_ETH`): the router has the native asset (from
/// `msg.value` or a prior step) and mints WETH to the recipient. We
/// model this as Burn(Router, Native) + Mint(recipient, WETH). Recipient
/// is whatever the calldata says — usually `ACTION_ADDRESS_THIS` (= Router)
/// so the wrapped asset stays staged for the next swap step.
fn apply_wrap(ledger: &mut Ledger, w: &WrapAction, ctx: &CallContext<'_>) {
    let Some(amount) = amount_spec_from(&w.amount) else {
        return;
    };
    ledger.apply(
        Effect::Burn {
            from: ActorRef::Router,
            asset: asset_from_ref(&w.native_asset),
            amount,
        },
        ctx,
    );
    ledger.apply(
        Effect::Mint {
            to: ActorRef::External(w.recipient.clone()),
            asset: asset_from_ref(&w.wrapped_asset),
            amount,
        },
        ctx,
    );
}

/// UnwrapAction (UR `UNWRAP_WETH`): the router holds WETH (from a prior
/// swap output) and unwraps it to native ETH for the recipient.
fn apply_unwrap(ledger: &mut Ledger, u: &UnwrapAction, ctx: &CallContext<'_>) {
    let Some(amount) = amount_spec_from(&u.amount) else {
        return;
    };
    ledger.apply(
        Effect::Burn {
            from: ActorRef::Router,
            asset: asset_from_ref(&u.wrapped_asset),
            amount,
        },
        ctx,
    );
    ledger.apply(
        Effect::Mint {
            to: ActorRef::External(u.recipient.clone()),
            asset: asset_from_ref(&u.native_asset),
            amount,
        },
        ctx,
    );
}

fn decide_swap_payer(ledger: &Ledger, token_in: &Asset, ctx: &CallContext<'_>) -> ActorRef {
    let router_actor = Ledger::resolve_actor(ActorRef::Router, ctx);
    let router_balance = ledger.balance(&router_actor, token_in);
    if router_balance.net.is_positive() {
        ActorRef::Router
    } else {
        ActorRef::User
    }
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
    if delta
        .iter()
        .any(|(_, b)| matches!(b.constraint, Constraint::Unknown))
    {
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
    let abs = unsigned_abs(bucket.net);
    let value = DecimalString::from_str(&abs.to_string()).ok();
    let kind = match (bucket.constraint, is_input) {
        (Constraint::Exact, _) => AmountKind::Exact,
        // Spending "at least X" → real spend can be ≥ X → cap is unknown,
        // surface as Max to keep policy evaluators conservative.
        (Constraint::AtLeast, true) => AmountKind::Max,
        (Constraint::AtLeast, false) => AmountKind::Min,
        (Constraint::AtMost, true) => AmountKind::Max,
        (Constraint::AtMost, false) => AmountKind::Min,
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

fn unsigned_abs(v: I256) -> U256 {
    if v.is_negative() {
        U256::from_str(v.to_string().trim_start_matches('-')).unwrap_or(U256::ZERO)
    } else {
        U256::from_str(&v.to_string()).unwrap_or(U256::ZERO)
    }
}
