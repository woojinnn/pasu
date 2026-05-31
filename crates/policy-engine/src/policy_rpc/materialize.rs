//! Policy-rpc system-failure verdict synthesis.
//!
//! The old `apply_rpc_results` projection path was removed in the Phase 1 action
//! restructure. The active ActionBody materialization path lives in
//! [`super::materialize_v2`]. The two surviving items below — the synthetic
//! [`SYSTEM_POLICY_ID`] and [`system_fail_verdict`] — are model-neutral and
//! shared by the v2 WASM evaluation surface.

use super::PolicyRpcError;
use crate::policy::{MatchedPolicy, PolicyRequestOrigin, Severity, Verdict};

/// D9 synthetic policy id.
///
/// Reported when a non-optional manifest requirement fails to materialize.
/// Callers that need a verdict-shaped result (rather than the raw
/// `PolicyRpcError`) should funnel `SystemFail` through
/// [`system_fail_verdict`].
pub const SYSTEM_POLICY_ID: &str = "__system__";

/// Translate `PolicyRpcError::SystemFail` into the D9 synthetic verdict.
///
/// Returns `Verdict::Fail` containing a single `MatchedPolicy` whose
/// `policy_id` is [`SYSTEM_POLICY_ID`] and whose `reason` is
/// `"rpc-unavailable: <call_id>"`. Returns `None` for any other variant so
/// the caller can propagate non-D9 failures as engine errors.
#[must_use]
pub fn system_fail_verdict(error: &PolicyRpcError) -> Option<Verdict> {
    if let PolicyRpcError::SystemFail { call_id, .. } = error {
        Some(Verdict::Fail(vec![MatchedPolicy {
            policy_id: SYSTEM_POLICY_ID.to_owned(),
            reason: Some(format!("rpc-unavailable: {call_id}")),
            severity: Severity::Deny,
            origin: PolicyRequestOrigin::Action,
        }]))
    } else {
        None
    }
}
