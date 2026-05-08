//! Host fact plan extraction.
//!
//! `required_host_facts(&Action) -> HostFactPlan` describes what host data
//! must be fetched before enrichment runs. The plan is the contract the
//! engine exposes to external orchestrators (notably the Chrome extension's
//! WASM bridge) so they can prefetch RPC reads and price quotes in parallel.
//!
//! Two tiers exist because windowing depends on already-stamped USD values:
//! - Tier 1: oracle, balances, allowances, clock — derivable from a bare Action.
//! - Tier 2: window keys — requires an `OracleSnapshot` because window keys
//!   are derived per-actor from USD-stamped enrichment output.

use crate::core::{Action, Address, OracleRequirement, Token};
use crate::host::oracle::SnapshotOracle;
use crate::host::stat_windows::StatKey;

/// Tier-1 host facts the engine needs from a precomputed snapshot.
///
/// Returned by [`required_host_facts`]. Each field enumerates a distinct
/// host capability lookup the snapshot must satisfy. Empty fields mean the
/// action does not require that capability.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostFactPlan {
    /// Tokens for which oracle USD prices are required.
    pub tokens_for_oracle: Vec<Token>,
    /// `(owner, token)` tuples for which `balanceOf(owner)` is required.
    pub balances: Vec<(Address, Token)>,
    /// `(owner, token, spender)` tuples for which `allowance(owner, spender)` is required.
    pub allowances: Vec<(Address, Token, Address)>,
    /// Whether evaluation requires the host clock (`nowTs` stamping).
    pub clock_required: bool,
    /// Signature-side oracle requirements that mirror DEX `oracle_requirements`.
    /// Used by the orchestrator when richer USD provenance metadata is desired
    /// (e.g., distinguishing "approve token X" vs "transfer token X").
    pub sig_oracle_requirements: Vec<OracleRequirement>,
}

/// Tier-2 host facts: window keys derivable only after USD enrichment.
///
/// Returned by [`required_window_keys`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowKeyPlan {
    /// Per-actor window keys to read from `StatWindows` before evaluation.
    pub keys: Vec<WindowKey>,
}

/// One key into the host's stat-window store.
///
/// Uses the engine's canonical `StatKey` newtype rather than a raw string
/// so that wire emission goes through `StatKey::as_str()` exactly once
/// (in the WASM bridge), and Rust code can match against `StatKey::*`
/// constants without typo risk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowKey {
    /// Wallet actor.
    pub actor: Address,
    /// Canonical stat key — see `crates/policy-engine/src/host/stat_windows.rs`.
    pub key: StatKey,
}

/// Tier-1 plan extraction. Pure function over a built Action.
#[must_use]
pub fn required_host_facts(_action: &Action) -> HostFactPlan {
    HostFactPlan::default()
}

/// Tier-2 plan extraction. Pure function over a built Action plus the
/// already-fetched oracle snapshot.
#[must_use]
pub fn required_window_keys(_action: &Action, _oracle: &SnapshotOracle) -> WindowKeyPlan {
    WindowKeyPlan::default()
}
