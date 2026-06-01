//! `cedar-wasm-lite` — minimal `cedar-policy` surface exposed via
//! `wasm-bindgen` for in-browser use.
//!
//! Used by `apps/web` to back the Editor's live syntax check and the
//! "test against TX" panel without round-tripping to the server. The
//! simulation-server crate previously hosted the same logic; with this
//! crate in place the server can drop its `cedar-policy` dependency and
//! its three editor-only routes (`/policies/validate`,
//! `/policies/:id/test`, `/simulate/sequence`).
//!
//! The exports are pure JSON-string in / JSON-string out so the TS side
//! doesn't need to know the wasm-bindgen ABI for complex types. Three
//! functions cover everything the editor needs:
//!
//! * `validate_policy(text)` — does `cedar_policy::PolicySet::from_str`
//!   accept the text?
//! * `test_policy(text, request, entities)` — `Authorizer::is_authorized`
//!   on a single ad-hoc Cedar request.
//! * `simulate_sequence(steps, policies)` — apply a batch of `[step,
//!   policy_set]` evaluations and roll up `pass`/`warn`/`fail`.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use std::str::FromStr;

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ── public DTOs (JSON-serialized at the wasm boundary) ──────────────────

#[derive(Serialize, Deserialize)]
pub struct ValidateResp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct CedarRequestInput {
    pub principal: String,
    pub action: String,
    pub resource: String,
    #[serde(default)]
    pub entities: serde_json::Value,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct TestResp {
    /// `"pass"` | `"warn"` | `"fail"`. `warn` is reserved for the
    /// `simulate_sequence` rollup; single-policy `test_policy` returns
    /// only `pass`/`fail`.
    pub verdict: String,
    pub matched: Vec<MatchedPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct MatchedPolicy {
    pub policy_id: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── wasm exports ────────────────────────────────────────────────────────

/// Install `console.error` panic hook so wasm panics surface in DevTools.
/// Called automatically when the wasm module is first instantiated.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

/// `validate_policy(text) -> JSON ValidateResp`. Mirrors the old
/// `POST /policies/validate` route — parse-only, no schema attached.
#[wasm_bindgen]
pub fn validate_policy(text: String) -> String {
    if text.trim().is_empty() {
        return json(&ValidateResp {
            ok: false,
            error: Some("cedar_text must not be empty".into()),
        });
    }
    let resp = match PolicySet::from_str(&text) {
        Ok(_) => ValidateResp {
            ok: true,
            error: None,
        },
        Err(e) => ValidateResp {
            ok: false,
            error: Some(e.to_string()),
        },
    };
    json(&resp)
}

/// `test_policy(text, request_json) -> JSON TestResp`. Mirrors the old
/// `POST /policies/:id/test` route — schema-less Authorizer over a
/// single ad-hoc Cedar request.
///
/// `request_json` must deserialize to `CedarRequestInput` (matching the
/// pre-existing FE shape: `principal`, `action`, `resource` as
/// `Type::"id"` strings, plus optional `entities` and `context`).
#[wasm_bindgen]
pub fn test_policy(text: String, request_json: String) -> String {
    let req: CedarRequestInput = match serde_json::from_str(&request_json) {
        Ok(r) => r,
        Err(e) => return err_resp(&format!("request JSON: {e}")),
    };

    let pset = match PolicySet::from_str(&text) {
        Ok(p) => p,
        Err(e) => return err_resp(&format!("policy parse: {e}")),
    };
    let cedar_req = match build_request(&req) {
        Ok(r) => r,
        Err(msg) => return err_resp(&msg),
    };
    let entities = match Entities::from_json_value(req.entities, None) {
        Ok(e) => e,
        Err(e) => return err_resp(&format!("entities: {e}")),
    };
    let resp = Authorizer::new().is_authorized(&cedar_req, &pset, &entities);
    json(&authorize_to_dto(&resp))
}

#[derive(Deserialize)]
pub struct SequenceStep {
    #[serde(default)]
    pub label: Option<String>,
    pub principal: String,
    pub action: String,
    pub resource: String,
    #[serde(default)]
    pub entities: serde_json::Value,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Deserialize)]
pub struct PolicyInput {
    pub policy_id: i64,
    pub policy_name: String,
    pub severity: String,
    pub cedar_text: String,
}

#[derive(Serialize, Deserialize)]
pub struct SequenceResp {
    pub overall: String,
    pub steps: Vec<SequenceStepResult>,
}

#[derive(Serialize, Deserialize)]
pub struct SequenceStepResult {
    pub label: Option<String>,
    pub verdict: String,
    pub policy_results: Vec<PolicyOutcome>,
}

#[derive(Serialize, Deserialize)]
pub struct PolicyOutcome {
    pub policy_id: i64,
    pub policy_name: String,
    pub severity: String,
    pub decision: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched: Vec<String>,
}

/// `simulate_sequence(steps_json, policies_json) -> JSON SequenceResp`.
/// Mirrors the old `POST /simulate/sequence` route. Each step is
/// evaluated against every supplied policy; per-step verdict is the
/// worst of {pass, warn, fail} across deny outcomes (warn-severity
/// policies count as warn, deny-severity count as fail). Overall is
/// the worst of the per-step verdicts.
#[wasm_bindgen]
pub fn simulate_sequence(steps_json: String, policies_json: String) -> String {
    let steps: Vec<SequenceStep> = match serde_json::from_str(&steps_json) {
        Ok(v) => v,
        Err(e) => return err_resp(&format!("steps JSON: {e}")),
    };
    let policies: Vec<PolicyInput> = match serde_json::from_str(&policies_json) {
        Ok(v) => v,
        Err(e) => return err_resp(&format!("policies JSON: {e}")),
    };

    // Pre-parse each policy once.
    let parsed: Vec<(PolicyInput, PolicySet)> = policies
        .into_iter()
        .filter_map(|p| match PolicySet::from_str(&p.cedar_text) {
            Ok(ps) => Some((p, ps)),
            Err(_) => None,
        })
        .collect();

    let auth = Authorizer::new();
    let mut step_results = Vec::with_capacity(steps.len());
    let mut any_fail = false;
    let mut any_warn = false;

    for step in steps {
        let cedar_req = match build_request_step(&step) {
            Ok(r) => r,
            Err(msg) => {
                step_results.push(SequenceStepResult {
                    label: step.label,
                    verdict: "fail".into(),
                    policy_results: vec![PolicyOutcome {
                        policy_id: -1,
                        policy_name: "__request_error__".into(),
                        severity: "deny".into(),
                        decision: "deny".into(),
                        matched: vec![msg],
                    }],
                });
                any_fail = true;
                continue;
            }
        };
        let entities = match Entities::from_json_value(step.entities, None) {
            Ok(e) => e,
            Err(e) => {
                step_results.push(SequenceStepResult {
                    label: step.label,
                    verdict: "fail".into(),
                    policy_results: vec![PolicyOutcome {
                        policy_id: -1,
                        policy_name: "__entities_error__".into(),
                        severity: "deny".into(),
                        decision: "deny".into(),
                        matched: vec![e.to_string()],
                    }],
                });
                any_fail = true;
                continue;
            }
        };

        let mut outcomes = Vec::with_capacity(parsed.len());
        let mut step_has_fail = false;
        let mut step_has_warn = false;
        for (pin, pset) in &parsed {
            let resp = auth.is_authorized(&cedar_req, pset, &entities);
            let decision = match resp.decision() {
                Decision::Allow => "allow",
                Decision::Deny => "deny",
            };
            let matched: Vec<String> = resp
                .diagnostics()
                .reason()
                .map(std::string::ToString::to_string)
                .collect();
            if decision == "deny" {
                match pin.severity.as_str() {
                    "warn" => step_has_warn = true,
                    _ => step_has_fail = true,
                }
            }
            outcomes.push(PolicyOutcome {
                policy_id: pin.policy_id,
                policy_name: pin.policy_name.clone(),
                severity: pin.severity.clone(),
                decision: decision.into(),
                matched,
            });
        }

        let step_verdict = if step_has_fail {
            "fail"
        } else if step_has_warn {
            "warn"
        } else {
            "pass"
        };
        if step_verdict == "fail" {
            any_fail = true;
        } else if step_verdict == "warn" {
            any_warn = true;
        }
        step_results.push(SequenceStepResult {
            label: step.label,
            verdict: step_verdict.into(),
            policy_results: outcomes,
        });
    }

    let overall = if any_fail {
        "fail"
    } else if any_warn {
        "warn"
    } else {
        "pass"
    };

    json(&SequenceResp {
        overall: overall.into(),
        steps: step_results,
    })
}

// ── helpers ─────────────────────────────────────────────────────────────

fn build_request(req: &CedarRequestInput) -> Result<Request, String> {
    let principal: EntityUid = req
        .principal
        .parse()
        .map_err(|e| format!("principal: {e}"))?;
    let action: EntityUid = req.action.parse().map_err(|e| format!("action: {e}"))?;
    let resource: EntityUid = req.resource.parse().map_err(|e| format!("resource: {e}"))?;
    let context =
        Context::from_json_value(req.context.clone(), None).map_err(|e| format!("context: {e}"))?;
    Request::new(principal, action, resource, context, None).map_err(|e| format!("request: {e}"))
}

fn build_request_step(step: &SequenceStep) -> Result<Request, String> {
    let principal: EntityUid = step
        .principal
        .parse()
        .map_err(|e| format!("principal: {e}"))?;
    let action: EntityUid = step.action.parse().map_err(|e| format!("action: {e}"))?;
    let resource: EntityUid = step
        .resource
        .parse()
        .map_err(|e| format!("resource: {e}"))?;
    let context = Context::from_json_value(step.context.clone(), None)
        .map_err(|e| format!("context: {e}"))?;
    Request::new(principal, action, resource, context, None).map_err(|e| format!("request: {e}"))
}

fn authorize_to_dto(response: &cedar_policy::Response) -> TestResp {
    let determining: Vec<String> = response
        .diagnostics()
        .reason()
        .map(std::string::ToString::to_string)
        .collect();
    let errors: Vec<String> = response
        .diagnostics()
        .errors()
        .map(std::string::ToString::to_string)
        .collect();
    let (verdict, matched) = match response.decision() {
        Decision::Allow => ("pass", Vec::new()),
        Decision::Deny if errors.is_empty() => (
            "fail",
            determining
                .iter()
                .map(|pid| MatchedPolicy {
                    policy_id: pid.clone(),
                    severity: "deny".into(),
                    reason: None,
                })
                .collect(),
        ),
        Decision::Deny => (
            "fail",
            errors
                .iter()
                .map(|e| MatchedPolicy {
                    policy_id: "__eval_error__".into(),
                    severity: "deny".into(),
                    reason: Some(e.clone()),
                })
                .collect(),
        ),
    };
    TestResp {
        verdict: verdict.into(),
        matched,
        error: None,
    }
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|e| format!(r#"{{"ok":false,"error":"{e}"}}"#))
}

fn err_resp(msg: &str) -> String {
    json(&TestResp {
        verdict: "fail".into(),
        matched: vec![],
        error: Some(msg.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_well_formed() {
        let r: ValidateResp = serde_json::from_str(&validate_policy(
            "permit(principal, action, resource);".into(),
        ))
        .unwrap();
        assert!(r.ok);
    }

    #[test]
    fn validate_rejects_garbage() {
        let r: ValidateResp =
            serde_json::from_str(&validate_policy("not cedar }}}".into())).unwrap();
        assert!(!r.ok);
        assert!(r.error.is_some());
    }

    #[test]
    fn test_policy_permit_all_allows() {
        let req = r#"{
            "principal": "Wallet::\"0xabc\"",
            "action": "Action::\"swap\"",
            "resource": "Protocol::\"0xdef\"",
            "entities": [],
            "context": {}
        }"#;
        let resp: TestResp = serde_json::from_str(&test_policy(
            "permit(principal, action, resource);".into(),
            req.into(),
        ))
        .unwrap();
        assert_eq!(resp.verdict, "pass");
    }

    #[test]
    fn test_policy_forbid_all_denies() {
        let req = r#"{
            "principal": "Wallet::\"0xabc\"",
            "action": "Action::\"swap\"",
            "resource": "Protocol::\"0xdef\"",
            "entities": [],
            "context": {}
        }"#;
        let resp: TestResp = serde_json::from_str(&test_policy(
            "forbid(principal, action, resource);".into(),
            req.into(),
        ))
        .unwrap();
        assert_eq!(resp.verdict, "fail");
    }

    #[test]
    fn simulate_sequence_rolls_up_worst() {
        let steps = r#"[
            { "label": "a", "principal": "Wallet::\"0xabc\"", "action": "Action::\"swap\"", "resource": "Protocol::\"0xdef\"", "entities": [], "context": {} },
            { "label": "b", "principal": "Wallet::\"0xabc\"", "action": "Action::\"swap\"", "resource": "Protocol::\"0xdef\"", "entities": [], "context": {} }
        ]"#;
        let policies = r#"[
            { "policy_id": 1, "policy_name": "permit-all", "severity": "info", "cedar_text": "permit(principal, action, resource);" },
            { "policy_id": 2, "policy_name": "forbid-warn", "severity": "warn", "cedar_text": "forbid(principal, action, resource);" }
        ]"#;
        let resp: SequenceResp =
            serde_json::from_str(&simulate_sequence(steps.into(), policies.into())).unwrap();
        // Both forbid (warn-severity) and permit (info) match → step ends warn.
        assert_eq!(resp.overall, "warn");
        assert_eq!(resp.steps.len(), 2);
        for s in &resp.steps {
            assert_eq!(s.verdict, "warn");
        }
    }
}
