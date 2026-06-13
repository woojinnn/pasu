//! Phase 5B — `evaluate_policy_request_json` WASM stub.
//!
//! The Phase 5 cutover wires the SW ↔ rpc-server JSON-RPC 2.0 channel
//! (`dambi.evaluate_v3`) — the rpc-server returns a `policy_request`
//! payload (typed actions + state_before/deltas/state_after) which the SW
//! then hands to this WASM entry alongside the user's Cedar policy set.
//!
//! Phase 5 = wire only. The Cedar engine integration (lowering each
//! `Action` into a Cedar `Request`, projecting state deltas into the
//! evaluation context, walking the user's policy set) is Phase 6's
//! responsibility. To unblock the SW → WASM round-trip today this
//! function returns a deterministic `Allow` verdict.
//!
//! The wire boundary is intentionally locked at Phase 5B so Phase 6 can
//! replace the body without touching any TS caller. The input JSON is
//! parsed-but-ignored on the stub (we still validate `input_too_large`
//! and `invalid_input_json` so callers get the same error shape they
//! will get once the real evaluator lands).

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::dto::{EngineErrorDto, Envelope};
use crate::exports::check_input_size;

/// Verdict kinds the Phase 5/6 surface exposes to TS.
///
/// `Allow` — Cedar produced no matched `forbid`. Wallet may proceed.
/// `Warn`  — Cedar matched a `warn`-severity policy. UI surfaces the
///           explanation and lets the user trust-and-proceed.
/// `Deny`  — Cedar matched a `deny`-severity `forbid`. Wallet aborts.
///
/// Mirrors the v1 `VerdictDto` (`pass`/`warn`/`fail`) but uses the new
/// PDF spec spellings the rpc-server / Cedar engine standardise on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyRequestVerdictKind {
    /// Cedar produced no matched forbid; wallet may proceed.
    Allow,
    /// Cedar matched a warn-severity policy; UI shows explanation and
    /// requires user trust-and-proceed.
    Warn,
    /// Cedar matched a deny-severity forbid; wallet must abort.
    Deny,
}

/// Verdict envelope returned by [`evaluate_policy_request_json`].
///
/// Phase 5B always emits `{ verdict: Allow, matched: [], reason: None }`
/// — the stub does not consult the input. Phase 6 fills `matched` with
/// the policy ids Cedar flagged + an optional human-readable reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequestVerdictDto {
    /// Top-level decision (allow / warn / deny).
    pub verdict: PolicyRequestVerdictKind,
    /// Policy ids that fired against this request.
    pub matched: Vec<String>,
    /// Optional human-readable explanation (engine error, matched
    /// policy reason, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PolicyRequestVerdictDto {
    fn allow_stub() -> Self {
        Self {
            verdict: PolicyRequestVerdictKind::Allow,
            matched: Vec::new(),
            reason: Some("phase-5b-stub: cedar wiring deferred to phase 6".to_owned()),
        }
    }
}

/// Phase 5B — evaluate a (policy_request, user_policies) pair.
///
/// **Input**: two JSON strings, ignored by the stub.
///   * `policy_request_json` — rpc-server's `dambi.evaluate_v3`
///     result body (`{ actions, state_before, deltas, state_after, ... }`).
///   * `user_policies_json` — caller-loaded Cedar policy set.
///
/// **Output**: the canonical `{ ok, data | error }` envelope wrapping a
/// [`PolicyRequestVerdictDto`]. Phase 5B always returns `Allow`.
///
/// **Errors**:
///   * `input_too_large` — either input exceeds the 4 MiB WASM budget.
///   * `invalid_input_json` — either input failed `serde_json::from_str`
///     to a generic `serde_json::Value` (we don't yet bind the shape).
///
/// Phase 6 replaces the body with the real Cedar evaluator. The wire
/// shape (both arguments + the verdict envelope) stays locked, so the
/// TS caller does not have to change.
#[wasm_bindgen]
pub fn evaluate_policy_request_json(
    policy_request_json: String,
    user_policies_json: String,
) -> String {
    let result = (|| -> Result<PolicyRequestVerdictDto, EngineErrorDto> {
        check_input_size(
            &policy_request_json,
            "evaluate_policy_request_json/policy_request",
        )?;
        check_input_size(
            &user_policies_json,
            "evaluate_policy_request_json/user_policies",
        )?;
        // Parse-but-ignore — we still want bad JSON to surface as
        // `invalid_input_json` (same kind the Phase 6 evaluator will
        // emit) instead of silently returning Allow.
        let _: serde_json::Value = serde_json::from_str(&policy_request_json).map_err(|error| {
            EngineErrorDto::new(
                "invalid_input_json",
                format!("invalid policy_request json: {error}"),
            )
        })?;
        let _: serde_json::Value = serde_json::from_str(&user_policies_json).map_err(|error| {
            EngineErrorDto::new(
                "invalid_input_json",
                format!("invalid user_policies json: {error}"),
            )
        })?;
        Ok(PolicyRequestVerdictDto::allow_stub())
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn stub_returns_allow_verdict_for_minimal_inputs() {
        let out = evaluate_policy_request_json(json!({}).to_string(), json!([]).to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["verdict"], "allow");
        assert_eq!(parsed["data"]["matched"], json!([]));
        assert!(parsed["data"]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("phase-5b-stub"));
    }

    #[test]
    fn stub_returns_allow_for_arbitrary_well_formed_payload() {
        let request = json!({
            "actions": [{ "body": { "domain": "token" } }],
            "state_before": {},
            "deltas": [],
            "state_after": {}
        });
        let policies = json!([{ "id": "user/no-large-swaps", "text": "forbid(...);" }]);
        let out = evaluate_policy_request_json(request.to_string(), policies.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["verdict"], "allow");
    }

    #[test]
    fn invalid_policy_request_json_is_surfaced() {
        let out = evaluate_policy_request_json("{not json".to_owned(), "[]".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json");
    }

    #[test]
    fn invalid_user_policies_json_is_surfaced() {
        let out = evaluate_policy_request_json("{}".to_owned(), "[not json".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json");
    }
}
