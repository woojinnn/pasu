//! Enrichment-fact host — the `facts/` namespace tree (sim-server fact host,
//! ADR-009; NOT the oracle/external policy-rpc daemon at :8787).
//!
//! This is a PROPOSAL mirroring the single-file Ground-pass
//! `crates/simulation/server/src/facts.rs`, refactored into a two-tier dispatch
//! so dozens of `planned` sim-server enrichment facts can be filled in by
//! different owners WITHOUT touching each other's code (conflict-free fill-in).
//!
//! ## Two-tier dispatch
//!
//! 1. This module's [`dispatch`] splits `method` on the first `.` to obtain the
//!    namespace (`approval`, `perp`, `token`, …) and routes the WHOLE method
//!    name to that namespace's `<ns>::dispatch`.
//! 2. Each `<ns>::dispatch` has one frozen arm per sim-server catalog method in
//!    that namespace plus an `UnknownMethod` catch-all, and calls the one
//!    private `fn` that implements (or stubs) that method.
//!
//! ## Conflict-free rules (FROZEN surfaces)
//!
//! - This top-level router is COMPLETE (all 17 namespaces) and FROZEN: it is
//!   never edited during fill-in.
//! - Each inner `<ns>::dispatch` match is FROZEN at scaffold time.
//! - Exactly one `fn` per method; fill in bodies only.
//!
//! See `facts-scaffold/README.md` for the owner work-breakdown and apply steps.

mod approval;
mod bridge;
mod claim;
mod curve;
mod deposit;
mod governance;
mod intent;
mod launchpad;
mod lp;
mod params;
mod permit;
mod perp;
mod portfolio;
mod position;
mod reserve;
mod signature;
mod stake;
mod token;
mod valuation;

use serde_json::Value;

use policy_state::WalletState;

/// Error from executing an enrichment fact against wallet state.
///
/// This is the SHARED error type for the whole `facts/` tree: the router and
/// every `<ns>` module route through it (`use super::FactError;`). The
/// [`FactError::NotImplemented`] variant is added relative to the Ground-pass
/// enum so the per-namespace stubs can return it from un-filled bodies while the
/// server still boots and serves the methods that ARE implemented.
#[derive(Debug, PartialEq, Eq)]
pub enum FactError {
    /// `spec.method` has no registered fact implementation.
    UnknownMethod(String),
    /// A required param was absent or the wrong JSON shape.
    BadParams(String),
    /// The fact is registered in a frozen dispatch but its body is still a
    /// scaffold stub. Lets the server boot and serve sibling methods.
    NotImplemented(String),
}

impl std::fmt::Display for FactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod(m) => write!(f, "unknown enrichment method `{m}`"),
            Self::BadParams(why) => write!(f, "bad enrichment params: {why}"),
            Self::NotImplemented(m) => write!(f, "enrichment method `{m}` is not yet implemented"),
        }
    }
}

impl std::error::Error for FactError {}

/// Evaluation context handed to every enrichment fact.
///
/// FROZEN signature carrier: facts take `&FactCtx`, never `&WalletState`
/// directly, so future fact inputs (post-apply `state_after`, evaluation `now`,
/// oracle/external feeds) are added as FIELDS here — additive, never a change to
/// the 54 per-method fn signatures.
pub struct FactCtx<'a> {
    /// Current (`State_1`) wallet-state snapshot.
    pub state: &'a WalletState,
}

/// Route an enrichment fact `method` to the namespace that owns it.
///
/// Splits `method` on the first `.` to obtain the namespace, then dispatches to
/// that namespace's `dispatch`. A method with no `.`, or one whose namespace has
/// no module here, falls through to [`FactError::UnknownMethod`].
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] for a name outside every registered
/// namespace, or propagates the namespace's per-method error
/// ([`FactError::NotImplemented`] for an un-filled stub, [`FactError::BadParams`]
/// for a malformed param payload).
///
/// FROZEN: one arm per namespace. Do not edit — fill in the per-method `fn`
/// bodies inside the `<ns>` modules instead.
pub fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let namespace = method.split('.').next().unwrap_or(method);
    match namespace {
        "approval" => approval::dispatch(method, params, ctx),
        "bridge" => bridge::dispatch(method, params, ctx),
        "claim" => claim::dispatch(method, params, ctx),
        "curve" => curve::dispatch(method, params, ctx),
        "deposit" => deposit::dispatch(method, params, ctx),
        "governance" => governance::dispatch(method, params, ctx),
        "intent" => intent::dispatch(method, params, ctx),
        "launchpad" => launchpad::dispatch(method, params, ctx),
        "lp" => lp::dispatch(method, params, ctx),
        "permit" => permit::dispatch(method, params, ctx),
        "perp" => perp::dispatch(method, params, ctx),
        "portfolio" => portfolio::dispatch(method, params, ctx),
        "position" => position::dispatch(method, params, ctx),
        "reserve" => reserve::dispatch(method, params, ctx),
        "signature" => signature::dispatch(method, params, ctx),
        "stake" => stake::dispatch(method, params, ctx),
        "token" => token::dispatch(method, params, ctx),
        "valuation" => valuation::dispatch(method, params, ctx),
        _ => Err(FactError::UnknownMethod(method.to_owned())),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn empty_state() -> WalletState {
        WalletState::new(policy_state::WalletId::new(
            "0x000000000000000000000000000000000000a01c"
                .parse()
                .unwrap(),
            [policy_state::primitives::ChainId::ethereum_mainnet()],
        ))
    }

    #[test]
    fn unknown_namespace_is_unknown_method() {
        let err = dispatch(
            "nope.whatever",
            &Value::Null,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }

    #[test]
    fn dotless_method_is_unknown_method() {
        let err = dispatch(
            "bareword",
            &Value::Null,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }

    #[test]
    fn known_namespace_unknown_method_is_unknown_method() {
        // Routes into perp::dispatch, whose catch-all rejects the unknown leaf.
        let err = dispatch(
            "perp.not_a_real_method",
            &Value::Null,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }

    #[test]
    fn registered_stub_routes_to_not_implemented() {
        // `curve.imbalance_skew` is registered in the frozen dispatch but is a
        // BLOCKED method (its input `action.live_inputs.pool_state.balances` is
        // dropped by the AMM lowering), so it still returns NotImplemented.
        let err = dispatch(
            "curve.imbalance_skew",
            &Value::Null,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
    }
}
