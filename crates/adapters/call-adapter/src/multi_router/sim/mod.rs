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
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::misc::{TransferAction, UnwrapAction, WrapAction};
use policy_engine::action::{
    Action, ActionEnvelope, AmountConstraint, AmountKind, AssetKind, AssetRef,
    AssetRefWithAmountConstraint, Category, DecimalString,
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
        Action::Transfer(t) => apply_transfer(ledger, t, ctx),
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
    let token_in = asset_from_ref(&s.input_token.asset);
    let token_out = asset_from_ref(&s.output_token.asset);

    let payer = decide_swap_payer(ledger, &token_in, ctx);
    if let Some(amount) = amount_spec_from(&s.input_token.amount) {
        ledger.apply(
            Effect::Burn {
                from: payer,
                asset: token_in,
                amount,
            },
            ctx,
        );
    }
    if let Some(amount) = amount_spec_from(&s.output_token.amount) {
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

/// WrapAction (UR `WRAP_ETH`): native ETH gets wrapped to WETH and minted
/// to `recipient` (typically `ACTION_ADDRESS_THIS` = Router so it stays
/// staged for the next swap step).
///
/// Payer choice mirrors `apply_swap`: if the router already holds native
/// (because msg.value was credited or a prior step transferred ETH to it),
/// the router pays. Otherwise the user pays — this covers `/api/decode`
/// callers that don't supply `value`, where the simulator would otherwise
/// burn from a router that holds nothing and lose the user's ETH loss
/// from the ledger entirely.
fn apply_wrap(ledger: &mut Ledger, w: &WrapAction, ctx: &CallContext<'_>) {
    let Some(amount) = amount_spec_from(&w.native_asset.amount) else {
        return;
    };
    let native = asset_from_ref(&w.native_asset.asset);
    let payer = decide_native_payer(ledger, &native, ctx);
    ledger.apply(
        Effect::Burn {
            from: payer,
            asset: native,
            amount,
        },
        ctx,
    );
    ledger.apply(
        Effect::Mint {
            to: ActorRef::External(w.recipient.clone()),
            asset: asset_from_ref(&w.wrapped_asset.asset),
            amount,
        },
        ctx,
    );
}

/// TransferAction (synthesised from UR `SWEEP` / `TRANSFER`): the router
/// hands `amount` of `token` to `recipient`. Modelled as
/// Move(t.from, t.recipient). When the from address resolves to neither
/// User nor Router (an external sender — rare here), we still apply it
/// so the ledger stays balanced and `interpret` falls back gracefully.
fn apply_transfer(ledger: &mut Ledger, t: &TransferAction, ctx: &CallContext<'_>) {
    let Some(amount) = amount_spec_from(&t.token.amount) else {
        return;
    };
    let from = if &t.from == ctx.from {
        ActorRef::User
    } else if &t.from == ctx.to {
        ActorRef::Router
    } else {
        ActorRef::External(t.from.clone())
    };
    ledger.apply(
        Effect::Move {
            from,
            to: ActorRef::External(t.recipient.clone()),
            asset: asset_from_ref(&t.token.asset),
            amount,
        },
        ctx,
    );
}

/// UnwrapAction (UR `UNWRAP_WETH`): WETH gets unwrapped to native ETH and
/// delivered to `recipient`.
///
/// Symmetric payer choice to `apply_wrap`: the router pays when it
/// already holds the wrapped asset (the typical post-swap case), the
/// user pays when it doesn't (rare: user calls UNWRAP_WETH directly
/// without a preceding swap that staged WETH on the router).
fn apply_unwrap(ledger: &mut Ledger, u: &UnwrapAction, ctx: &CallContext<'_>) {
    let Some(amount) = amount_spec_from(&u.wrapped_asset.amount) else {
        return;
    };
    let wrapped = asset_from_ref(&u.wrapped_asset.asset);
    let payer = decide_native_payer(ledger, &wrapped, ctx);
    ledger.apply(
        Effect::Burn {
            from: payer,
            asset: wrapped,
            amount,
        },
        ctx,
    );
    ledger.apply(
        Effect::Mint {
            to: ActorRef::External(u.recipient.clone()),
            asset: asset_from_ref(&u.native_asset.asset),
            amount,
        },
        ctx,
    );
}

/// Generic payer-decision used by `apply_swap`, `apply_wrap`, and
/// `apply_unwrap`: prefer the router when it already holds the asset
/// (a prior step staged it there), fall back to the user otherwise.
///
/// Why "prefer router when balance is positive": Universal-Router-style
/// flows almost always pre-stage the input asset on the router (via
/// msg.value, WRAP_ETH, PERMIT2_TRANSFER_FROM, or a prior swap output).
/// When the simulator sees that staging, it should treat the next
/// consuming action as draining the router. When it doesn't (because
/// /api/decode didn't pass `value`, or the call has no preceding stage),
/// the safest assumption is "the user pays" — which matches direct
/// `transferFrom` flows and keeps the user's loss visible in the ledger.
fn decide_native_payer(ledger: &Ledger, asset: &Asset, ctx: &CallContext<'_>) -> ActorRef {
    let router_actor = Ledger::resolve_actor(ActorRef::Router, ctx);
    let router_balance = ledger.balance(&router_actor, asset);
    if router_balance.net.is_positive() {
        ActorRef::Router
    } else {
        ActorRef::User
    }
}

/// `apply_swap` shim — kept as a named export so the call site reads
/// "decide_swap_payer" intentionally even though the logic is shared.
fn decide_swap_payer(ledger: &Ledger, token_in: &Asset, ctx: &CallContext<'_>) -> ActorRef {
    decide_native_payer(ledger, token_in, ctx)
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
            // mode). If none exists, skip the merge — the simulator can
            // describe the asset flow but shouldn't invent a swap from
            // nothing.
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
        swap_mode: derive_mode(
            spent_bucket.constraint,
            received_bucket.constraint,
            template.swap_mode,
        ),
        input_token: AssetRefWithAmountConstraint {
            asset: asset_to_ref(spent_asset),
            amount: amount_in,
        },
        output_token: AssetRefWithAmountConstraint {
            asset: asset_to_ref(received_asset),
            amount: amount_out,
        },
        recipient: ctx.from.clone(),
        validity: template.validity,
        fee_bps: template.fee_bps,
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

fn asset_to_ref(a: Asset) -> AssetRef {
    match a {
        Asset::Native => AssetRef {
            kind: AssetKind::Native,
            address: None,
            token_id: None,
            symbol: Some("ETH".to_owned()),
            decimals: Some(18),
        },
        Asset::Erc20(addr) => AssetRef {
            kind: AssetKind::Erc20,
            address: Some(addr),
            token_id: None,
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
