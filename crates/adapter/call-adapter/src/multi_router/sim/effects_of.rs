//! Translate `ActionEnvelope`s into the simulator's `Effect` vocabulary.
//!
//! Each envelope variant maps to zero or more effects describing how the
//! asset balances move. Envelopes that don't move assets (Approve, Permit,
//! …) yield an empty list — they simply don't affect the ledger.
//!
//! Adding a new envelope variant is a single match arm here; the
//! `Ledger` and engine stay untouched.

use std::str::FromStr as _;

use alloy_primitives::U256;
use policy_engine::action::dex::SwapAction;
use policy_engine::action::misc::{UnwrapAction, WrapAction};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
};

use super::effect::{ActorRef, Asset, AmountSpec, Effect};

/// Map an envelope to the asset movements it implies. Empty list = no
/// ledger impact (Approve, Permit, sign-only actions, etc.).
pub(in crate::multi_router) fn effects_of(env: &ActionEnvelope) -> Vec<Effect> {
    match &env.action {
        Action::Swap(s) => effects_of_swap(s),
        Action::Wrap(w) => effects_of_wrap(w),
        Action::Unwrap(u) => effects_of_unwrap(u),
        // Other variants (Approve, Permit, lending actions, …) don't move
        // assets at this layer — they're either authorisations or modeled
        // by separate effects we don't synthesise from envelope alone.
        _ => Vec::new(),
    }
}

fn effects_of_swap(s: &SwapAction) -> Vec<Effect> {
    // A wallet-side swap is "user spends token_in and receives token_out".
    // Payer is the user (the wallet owner whose calldata this is) — when
    // the actual on-chain payer is the router (e.g. UR's `payerIsUser=false`
    // path), the SETTLE/TAKE companion opcodes inside the same multi-call
    // re-balance the ledger to the same net delta, so this default is safe.
    let mut out = Vec::with_capacity(2);
    if let Some(amount) = amount_spec_from(&s.amount_in) {
        out.push(Effect::Burn {
            from: ActorRef::User,
            asset: asset_from_ref(&s.token_in),
            amount,
        });
    }
    if let Some(amount) = amount_spec_from(&s.amount_out) {
        out.push(Effect::Mint {
            to: ActorRef::External(s.recipient.clone()),
            asset: asset_from_ref(&s.token_out),
            amount,
        });
    }
    out
}

fn effects_of_wrap(w: &WrapAction) -> Vec<Effect> {
    // WRAP_ETH on Universal Router pulls native ETH from the router (which
    // already holds the user's msg.value) and mints WETH to the recipient.
    // Modeling the burn against the router matches what actually happens
    // on-chain; the user's native loss was recorded by the msg.value
    // initialisation in `simulate`.
    let Some(amount) = amount_spec_from(&w.amount) else {
        return Vec::new();
    };
    vec![
        Effect::Burn {
            from: ActorRef::Router,
            asset: asset_from_ref(&w.native_asset),
            amount,
        },
        Effect::Mint {
            to: ActorRef::External(w.recipient.clone()),
            asset: asset_from_ref(&w.wrapped_asset),
            amount,
        },
    ]
}

fn effects_of_unwrap(u: &UnwrapAction) -> Vec<Effect> {
    let Some(amount) = amount_spec_from(&u.amount) else {
        return Vec::new();
    };
    vec![
        Effect::Burn {
            from: ActorRef::Router,
            asset: asset_from_ref(&u.wrapped_asset),
            amount,
        },
        Effect::Mint {
            to: ActorRef::External(u.recipient.clone()),
            asset: asset_from_ref(&u.native_asset),
            amount,
        },
    ]
}

/// Translate `AssetRef` (policy-engine schema) to the simulator's `Asset`.
/// Native (no address) → `Asset::Native`; ERC-20 with address → `Asset::Erc20`.
/// Anything else (NFTs, missing address) is treated as Native — the simulator
/// only models fungible asset accounting today.
pub(in crate::multi_router) fn asset_from_ref(r: &AssetRef) -> Asset {
    match (&r.kind, &r.address) {
        (AssetKind::Native, _) => Asset::Native,
        (AssetKind::Erc20, Some(addr)) => Asset::Erc20(addr.clone()),
        // ERC-20 missing an address shouldn't happen, but degrade gracefully.
        _ => Asset::Native,
    }
}

/// Translate `AmountConstraint` to `AmountSpec`. Returns `None` when the
/// amount has no usable value (e.g. `Unlimited` permits — the simulator
/// can't reason about an open-ended bound, so the envelope just yields
/// no effect for that side).
pub(in crate::multi_router) fn amount_spec_from(c: &AmountConstraint) -> Option<AmountSpec> {
    let value = c.value.as_ref()?;
    let parsed = U256::from_str(&value.to_string()).ok()?;
    Some(match c.kind {
        AmountKind::Exact => AmountSpec::Exact(parsed),
        AmountKind::Min => AmountSpec::AtLeast(parsed),
        AmountKind::Max => AmountSpec::AtMost(parsed),
        // Unlimited is a permit-side thing; for accounting we treat it as
        // AtMost(MAX), which the interpreter will surface as Unknown.
        AmountKind::Unlimited => AmountSpec::AtMost(parsed),
        // Estimated and Unknown carry no firm bound; the simulator can't
        // reason about them — return None so the bucket isn't poisoned.
        AmountKind::Estimated | AmountKind::Unknown => return None,
    })
}

/// Address-equality helper used by the interpret step. Kept here so the
/// resolution rules stay alongside `Effect` semantics.
#[allow(dead_code)]
pub(in crate::multi_router) fn actor_ref_for(address: &Address, ctx_from: &Address, ctx_to: &Address) -> ActorRef {
    if address == ctx_from {
        ActorRef::User
    } else if address == ctx_to {
        ActorRef::Router
    } else {
        ActorRef::External(address.clone())
    }
}
