//! Conversion helpers used by the simulator (`super::simulate`) when reading
//! values out of `ActionEnvelope` fields.
//!
//! Per-envelope dispatch (which actor pays / receives) lives in `mod.rs`
//! because it depends on ledger state, not just envelope content. This
//! file just stays pure value translation.

use std::str::FromStr as _;

use alloy_primitives::U256;
use policy_engine::action::{AmountConstraint, AmountKind, AssetKind, AssetRef};

use super::effect::{Asset, AmountSpec};

/// Translate `AssetRef` (policy-engine schema) to the simulator's `Asset`.
/// Native (no address) → `Asset::Native`; ERC-20 with address → `Asset::Erc20`.
/// Anything else (NFTs, missing address) is treated as Native — the simulator
/// only models fungible asset accounting today.
pub(super) fn asset_from_ref(r: &AssetRef) -> Asset {
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
pub(super) fn amount_spec_from(c: &AmountConstraint) -> Option<AmountSpec> {
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
        // Portion is a percentage-of-balance constraint (used by V4 TAKE_PORTION
        // and similar) that has no firm bound until the surrounding flow
        // resolves, so treat it the same way.
        AmountKind::Estimated | AmountKind::Unknown | AmountKind::Portion => return None,
    })
}
