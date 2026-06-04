//! Denial diagnosis: Cedar-probe oracle. See
//! docs/superpowers/specs/2026-06-05-blockir-denial-diagnosis-design.md.

use std::collections::BTreeMap;
use std::str::FromStr;

use cedar_policy::{
    AuthorizationError, Authorizer, Context, Entities, EntityUid, Policy, PolicyId, PolicySet,
    Request,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::wasm_bindgen;

use policy_engine::lowering_v2::LoweredAction;
use policy_transition::action::{ActionBody, ActionMeta};

use crate::action_eval_exports::{materialized_context, BundleInput, TxInput};
use crate::dto::{EngineErrorDto, Envelope};
use crate::exports::check_input_size;

/// One probe: a permit policy (as EST) whose `when` body is a boolean subtree
/// of the denying policy, identified by its structural node path.
#[derive(Debug, Deserialize)]
struct ProbeInput {
    id: String,
    est: Value,
}

#[derive(Debug, Deserialize)]
struct DiagnosisInput {
    action: ActionBody,
    meta: ActionMeta,
    tx: TxInput,
    #[serde(default)]
    bundles: Vec<BundleInput>,
    #[serde(default)]
    results: BTreeMap<String, Value>,
    probes: Vec<ProbeInput>,
}

#[derive(Debug, Serialize)]
struct DiagnosisOutput {
    /// Probe ids whose body evaluated TRUE (Cedar `reason()`).
    true_ids: Vec<String>,
    /// Probe ids whose body ERRORED (Cedar `errors()`); every other id is false.
    error_ids: Vec<String>,
}

/// Run all probes once against the denying request's materialized context using a
/// RAW `Authorizer` (NOT `build_from_per_policy`, whose fail-closed net would
/// corrupt an errored probe). Returns which probe bodies were true / errored.
#[wasm_bindgen]
#[must_use]
pub fn run_diagnosis_probes_v2_json(input_json: String) -> String {
    let result = (|| -> Result<DiagnosisOutput, EngineErrorDto> {
        check_input_size(&input_json, "run_diagnosis_probes_v2_json")?;
        let input: DiagnosisInput = serde_json::from_str(&input_json)
            .map_err(|e| EngineErrorDto::new("invalid_input_json", e.to_string()))?;

        let (lowered, context) = materialized_context(
            &input.action, &input.meta, &input.tx, &input.bundles, &input.results,
        )?;

        run_probes(&lowered, &context, &input.probes)
    })();

    match result {
        Ok(out) => Envelope::ok(out).to_json(),
        Err(e) => Envelope::<()>::err(e.kind, e.message).to_json(),
    }
}

fn run_probes(
    lowered: &LoweredAction,
    context: &Value,
    probes: &[ProbeInput],
) -> Result<DiagnosisOutput, EngineErrorDto> {
    let principal: EntityUid = lowered.principal.parse()
        .map_err(|e: cedar_policy::ParseErrors| EngineErrorDto::new("principal", e.to_string()))?;
    let action: EntityUid = lowered.action_uid.parse()
        .map_err(|e: cedar_policy::ParseErrors| EngineErrorDto::new("action", e.to_string()))?;
    let resource: EntityUid = lowered.resource.parse()
        .map_err(|e: cedar_policy::ParseErrors| EngineErrorDto::new("resource", e.to_string()))?;

    // Schema-less, empty entities — mirrors the per-policy eval path (engine.rs:227-236, :318).
    let cedar_ctx = Context::from_json_value(context.clone(), None)
        .map_err(|e| EngineErrorDto::new("context", e.to_string()))?;
    let request = Request::new(principal, action, resource, cedar_ctx, None)
        .map_err(|e| EngineErrorDto::new("request", e.to_string()))?;
    let entities = Entities::from_json_value(Value::Array(Vec::new()), None)
        .map_err(|e| EngineErrorDto::new("entities", e.to_string()))?;

    let mut set = PolicySet::new();
    for p in probes {
        // PolicyId parse is infallible in cedar 4.10.
        let pid = PolicyId::from_str(&p.id).expect("PolicyId parse is infallible");
        let policy = Policy::from_json(Some(pid), p.est.clone())
            .map_err(|e| EngineErrorDto::new("probe_parse", format!("{}: {e}", p.id)))?;
        set.add(policy)
            .map_err(|e| EngineErrorDto::new("probe_add", format!("{}: {e}", p.id)))?;
    }

    let resp = Authorizer::new().is_authorized(&request, &set, &entities);
    let true_ids: Vec<String> = resp.diagnostics().reason().map(ToString::to_string).collect();
    // cedar 4.10: `AuthorizationError` is an enum (no `.id()`); its single
    // `PolicyEvaluationError` variant carries `policy_id()` (pinned in Task 1's spike).
    let error_ids: Vec<String> = resp
        .diagnostics()
        .errors()
        .map(|e| match e {
            AuthorizationError::PolicyEvaluationError(pe) => pe.policy_id().to_string(),
        })
        .collect();

    Ok(DiagnosisOutput { true_ids, error_ids })
}

#[cfg(test)]
mod spike {
    use cedar_policy::{
        AuthorizationError, Authorizer, Context, Entities, EntityUid, Policy, PolicyId, PolicySet,
        Request,
    };
    use serde_json::json;
    use std::str::FromStr;

    /// S1: `reason()` returns ALL satisfied permits on Allow.
    /// S2: a permit built from EST via `Policy::from_json` parses + evaluates
    ///     schema-less, and an errored body shows up in `errors()` (not panic).
    #[test]
    fn probe_oracle_apis_behave() {
        // context: { "slippageBp": 150 }
        let context =
            Context::from_json_value(json!({ "slippageBp": 150 }), None).expect("ctx");
        let principal: EntityUid = "Wallet::\"w\"".parse().unwrap();
        let action: EntityUid = "Amm::Action::\"Swap\"".parse().unwrap();
        let resource: EntityUid = "Protocol::\"p\"".parse().unwrap();
        let request =
            Request::new(principal, action, resource, context, None).expect("request");
        let entities = Entities::from_json_value(json!([]), None).expect("entities");

        // Two probes built FROM EST (the shape blocksToEst emits):
        //   c0_true : permit when { context.slippageBp > 100 }   -> TRUE
        //   c0_false: permit when { context.slippageBp > 1000 }  -> false
        //   c0_err  : permit when { context.missing > 1 }        -> ERROR (no such attr)
        let est_gt = |rhs: i64| {
            json!({
                "effect": "permit",
                "principal": { "op": "All" },
                "action": { "op": "All" },
                "resource": { "op": "All" },
                "conditions": [{ "kind": "when", "body":
                    { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "slippageBp" } },
                             "right": { "Value": rhs } } } }]
            })
        };
        let est_err = json!({
            "effect": "permit",
            "principal": { "op": "All" }, "action": { "op": "All" }, "resource": { "op": "All" },
            "conditions": [{ "kind": "when", "body":
                { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "missing" } },
                         "right": { "Value": 1 } } } }]
        });

        let mut set = PolicySet::new();
        set.add(Policy::from_json(Some(PolicyId::from_str("c0_true").unwrap()), est_gt(100)).unwrap()).unwrap();
        set.add(Policy::from_json(Some(PolicyId::from_str("c0_false").unwrap()), est_gt(1000)).unwrap()).unwrap();
        set.add(Policy::from_json(Some(PolicyId::from_str("c0_err").unwrap()), est_err).unwrap()).unwrap();

        let resp = Authorizer::new().is_authorized(&request, &set, &entities);

        let true_ids: Vec<String> =
            resp.diagnostics().reason().map(ToString::to_string).collect();
        assert!(true_ids.contains(&"c0_true".to_string()), "true probe in reason()");
        assert!(!true_ids.contains(&"c0_false".to_string()), "false probe NOT in reason()");

        // S2: the errored probe is reported, not fatal. cedar 4.10's
        // `AuthorizationError` is an enum (no `.id()` accessor); its single
        // `PolicyEvaluationError` variant exposes `policy_id()`.
        let error_ids: Vec<String> = resp
            .diagnostics()
            .errors()
            .map(|e| match e {
                AuthorizationError::PolicyEvaluationError(pe) => pe.policy_id().to_string(),
            })
            .collect();
        assert!(error_ids.contains(&"c0_err".to_string()), "errored probe id in errors()");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // Reuse the swap fixture builder from the evaluate export's tests by
    // duplicating the minimal swap_sample shape we need: a swap with a chosen
    // slippage. (Kept local so this test module is self-contained.)
    fn swap_input(slippage_bp: u32, probes: Value) -> String {
        // The action/meta come from action_eval_exports' swap_sample via a tiny
        // JSON mirror is brittle; instead drive through the public builder.
        let (body, meta) = crate::action_eval_exports::tests::swap_sample_with_slippage(slippage_bp);
        json!({
            "action": body,
            "meta": meta,
            "tx": { "chain_id": "eip155:42161",
                    "from": "0x1111111111111111111111111111111111111111",
                    "to":   "0x2222222222222222222222222222222222222222" },
            "bundles": [],
            "results": {},
            "probes": probes
        })
        .to_string()
    }

    #[test]
    fn diagnoses_true_and_false_slippage_probes() {
        // Probe set: c0 = (slippageBp > 100). The fixture's slippage 150 trips it.
        let probes = json!([
            { "id": "c0", "est": {
                "effect": "permit",
                "principal": { "op": "All" }, "action": { "op": "All" }, "resource": { "op": "All" },
                "conditions": [{ "kind": "when", "body":
                    { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "slippageBp" } },
                             "right": { "Value": 100 } } } }] } }
        ]);

        let out = run_diagnosis_probes_v2_json(swap_input(150, probes.clone()));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["true_ids"], json!(["c0"]), "150 > 100 is true: {parsed}");

        let out2 = run_diagnosis_probes_v2_json(swap_input(50, probes));
        let parsed2: Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(parsed2["data"]["true_ids"], json!([]), "50 > 100 is false: {parsed2}");
    }
}
