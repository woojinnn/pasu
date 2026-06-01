//! Editor-facing Cedar exports — `validate_policy_text`,
//! `test_policy_text`, `simulate_policy_sequence`.
//!
//! Background: the policy-server used to host `POST
//! /policies/validate` + `POST /policies/:id/test` + `POST
//! /simulate/sequence`. The DB-only server contract pushed those out
//! of the server crate; the apps/web Editor and the
//! Simulation/test-bench page now route through the browser
//! extension instead. The extension already loads
//! `policy-engine-wasm` to evaluate real transactions; adding three
//! schema-less wrappers here lets the same wasm module serve both
//! the live action path (`evaluate_action_v2_json`) and the
//! editor's Cedar Authorizer needs without a separate wasm bundle.
//!
//! All exports are JSON-string in / JSON-string out so the TS side
//! stays free of `wasm-bindgen` ABI plumbing.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use std::str::FromStr;

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ── DTOs ────────────────────────────────────────────────────────────────

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

// ── exports ─────────────────────────────────────────────────────────────

/// `validate_policy_text(text) -> JSON ValidateResp`. Parse-only;
/// no schema attached so authors can iterate freely.
#[wasm_bindgen]
pub fn validate_policy_text(text: String) -> String {
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

/// `test_policy_text(text, request_json) -> JSON TestResp`. Schema-less
/// Authorizer over a single ad-hoc Cedar request — the Editor's
/// "test against TX" panel calls this with the current draft text.
#[wasm_bindgen]
pub fn test_policy_text(text: String, request_json: String) -> String {
    let req: CedarRequestInput = match serde_json::from_str(&request_json) {
        Ok(r) => r,
        Err(e) => return err_test(&format!("request JSON: {e}")),
    };

    let pset = match PolicySet::from_str(&text) {
        Ok(p) => p,
        Err(e) => return err_test(&format!("policy parse: {e}")),
    };
    let cedar_req = match build_request(&req) {
        Ok(r) => r,
        Err(msg) => return err_test(&msg),
    };
    let entities = match Entities::from_json_value(req.entities, None) {
        Ok(e) => e,
        Err(e) => return err_test(&format!("entities: {e}")),
    };
    let resp = Authorizer::new().is_authorized(&cedar_req, &pset, &entities);
    json(&authorize_to_dto(&resp))
}

/// `simulate_policy_sequence(steps_json, policies_json) -> JSON SequenceResp`.
/// Fan-out: every step × every supplied policy. Per-step verdict is
/// `fail` if any deny-severity policy denies, `warn` if any warn-
/// severity policy denies, else `pass`. Overall is the worst per-step.
#[wasm_bindgen]
pub fn simulate_policy_sequence(steps_json: String, policies_json: String) -> String {
    let steps: Vec<SequenceStep> = match serde_json::from_str(&steps_json) {
        Ok(v) => v,
        Err(e) => return err_test(&format!("steps JSON: {e}")),
    };
    let policies: Vec<PolicyInput> = match serde_json::from_str(&policies_json) {
        Ok(v) => v,
        Err(e) => return err_test(&format!("policies JSON: {e}")),
    };

    let parsed: Vec<(PolicyInput, PolicySet)> = policies
        .into_iter()
        .filter_map(|p| PolicySet::from_str(&p.cedar_text).ok().map(|ps| (p, ps)))
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

fn err_test(msg: &str) -> String {
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
        let r: ValidateResp = serde_json::from_str(&validate_policy_text(
            "permit(principal, action, resource);".into(),
        ))
        .unwrap();
        assert!(r.ok);
    }

    #[test]
    fn validate_rejects_garbage() {
        let r: ValidateResp =
            serde_json::from_str(&validate_policy_text("not cedar }}}".into())).unwrap();
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
        let resp: TestResp = serde_json::from_str(&test_policy_text(
            "permit(principal, action, resource);".into(),
            req.into(),
        ))
        .unwrap();
        assert_eq!(resp.verdict, "pass");
    }
}
