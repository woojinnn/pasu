//! Denial diagnosis: Cedar-probe oracle (the back end of the "which sub-clause
//! caused this denial?" feature).
//!
//! ## How it is invoked
//!
//! The TS side (`browser-extension/dashboard/src/cedar/diagnosis`) builds the
//! probes and calls this through the service worker:
//!
//! ```text
//! dashboard  runDiagnosisProbes(req)            (server-api/diagnosis.ts)
//!   → SW op  "run-diagnosis-probes"             (service-worker/index.ts)
//!   → bridge runDiagnosisProbesV2(inputJson)    (service-worker/wasm-bridge.ts)
//!   → WASM   run_diagnosis_probes_v2_json(...)  (this file)
//! ```
//!
//! The frontend usage guide is `cedar/diagnosis/README.md` in the dashboard
//! package; this module is the runner it ultimately reaches.
//!
//! ## Contract
//!
//! Input JSON (`DiagnosisInput`): the same `{ action, meta, tx, bundles, results }`
//! the evaluate path takes, PLUS `probes: [{ id, est }]` — each probe is a
//! `permit(...) when { <subtree> }` policy (as EST) authored in TS, whose `id` is
//! the subtree's structural node path. Output JSON (`DiagnosisOutput`):
//! `{ true_ids, error_ids }` — the probe ids whose body evaluated TRUE / ERRORED.
//! Every other probe id is implicitly false; the TS blame walker derives culprits.
//!
//! ## Why it is provable
//!
//! `materialized_context` rebuilds the byte-identical context the real verdict
//! used (lower → plan → materialize), and we run a RAW `Authorizer` ONCE over the
//! probes against it — so each probe's truth is computed by *Cedar itself* on the
//! *same* context, and cannot disagree with the real verdict. No second evaluator.

use std::collections::BTreeMap;

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
///
/// INVARIANT: every probe MUST be a `permit` policy — the `reason() ⇒ true`
/// mapping in [`run_probes`] depends on it (a satisfied `forbid` would invert
/// the polarity and not appear in `reason()`).
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
            &input.action,
            &input.meta,
            &input.tx,
            &input.bundles,
            &input.results,
        )?;

        run_probes(&lowered, context, &input.probes)
    })();

    match result {
        Ok(out) => Envelope::ok(out).to_json(),
        Err(e) => Envelope::<()>::err(e.kind, e.message).to_json(),
    }
}

fn run_probes(
    lowered: &LoweredAction,
    context: Value,
    probes: &[ProbeInput],
) -> Result<DiagnosisOutput, EngineErrorDto> {
    let principal: EntityUid = lowered
        .principal
        .parse()
        .map_err(|e: cedar_policy::ParseErrors| EngineErrorDto::new("principal", e.to_string()))?;
    let action: EntityUid = lowered
        .action_uid
        .parse()
        .map_err(|e: cedar_policy::ParseErrors| EngineErrorDto::new("action", e.to_string()))?;
    let resource: EntityUid = lowered
        .resource
        .parse()
        .map_err(|e: cedar_policy::ParseErrors| EngineErrorDto::new("resource", e.to_string()))?;

    // Schema-less, empty entities — mirrors the per-policy eval path (engine.rs:227-236, :318).
    let cedar_ctx = Context::from_json_value(context, None)
        .map_err(|e| EngineErrorDto::new("context", e.to_string()))?;
    let request = Request::new(principal, action, resource, cedar_ctx, None)
        .map_err(|e| EngineErrorDto::new("request", e.to_string()))?;
    let entities = Entities::from_json_value(Value::Array(Vec::new()), None)
        .map_err(|e| EngineErrorDto::new("entities", e.to_string()))?;

    let mut set = PolicySet::new();
    for p in probes {
        let pid = PolicyId::new(&p.id);
        let policy = Policy::from_json(Some(pid), p.est.clone())
            .map_err(|e| EngineErrorDto::new("probe_parse", format!("{}: {e}", p.id)))?;
        set.add(policy)
            .map_err(|e| EngineErrorDto::new("probe_add", format!("{}: {e}", p.id)))?;
    }

    let resp = Authorizer::new().is_authorized(&request, &set, &entities);
    let true_ids: Vec<String> = resp
        .diagnostics()
        .reason()
        .map(ToString::to_string)
        .collect();
    // cedar 4.10: `AuthorizationError` is an enum (no `.id()`); its single
    // `PolicyEvaluationError` variant carries `policy_id()` (pinned in Task 1's spike).
    let error_ids: Vec<String> = resp
        .diagnostics()
        .errors()
        .map(|e| match e {
            AuthorizationError::PolicyEvaluationError(pe) => pe.policy_id().to_string(),
        })
        .collect();

    Ok(DiagnosisOutput {
        true_ids,
        error_ids,
    })
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
        let context = Context::from_json_value(json!({ "slippageBp": 150 }), None).expect("ctx");
        let principal: EntityUid = "Wallet::\"w\"".parse().unwrap();
        let action: EntityUid = "Amm::Action::\"Swap\"".parse().unwrap();
        let resource: EntityUid = "Protocol::\"p\"".parse().unwrap();
        let request = Request::new(principal, action, resource, context, None).expect("request");
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
        set.add(
            Policy::from_json(Some(PolicyId::from_str("c0_true").unwrap()), est_gt(100)).unwrap(),
        )
        .unwrap();
        set.add(
            Policy::from_json(Some(PolicyId::from_str("c0_false").unwrap()), est_gt(1000)).unwrap(),
        )
        .unwrap();
        set.add(Policy::from_json(Some(PolicyId::from_str("c0_err").unwrap()), est_err).unwrap())
            .unwrap();

        let resp = Authorizer::new().is_authorized(&request, &set, &entities);

        let true_ids: Vec<String> = resp
            .diagnostics()
            .reason()
            .map(ToString::to_string)
            .collect();
        assert!(
            true_ids.contains(&"c0_true".to_string()),
            "true probe in reason()"
        );
        assert!(
            !true_ids.contains(&"c0_false".to_string()),
            "false probe NOT in reason()"
        );

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
        assert!(
            error_ids.contains(&"c0_err".to_string()),
            "errored probe id in errors()"
        );
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
        let (body, meta) =
            crate::action_eval_exports::tests::swap_sample_with_slippage(slippage_bp);
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
        assert_eq!(
            parsed["data"]["true_ids"],
            json!(["c0"]),
            "150 > 100 is true: {parsed}"
        );

        let out2 = run_diagnosis_probes_v2_json(swap_input(50, probes));
        let parsed2: Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(
            parsed2["data"]["true_ids"],
            json!([]),
            "50 > 100 is false: {parsed2}"
        );
    }

    #[test]
    fn errored_probe_is_reported_not_fatal() {
        // Probe references a missing context attr → errors(), not a panic, not true.
        let probes = json!([
            { "id": "c0", "est": {
                "effect": "permit",
                "principal": { "op": "All" }, "action": { "op": "All" }, "resource": { "op": "All" },
                "conditions": [{ "kind": "when", "body":
                    { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "nope" } },
                             "right": { "Value": 1 } } } }] } }
        ]);
        let out = run_diagnosis_probes_v2_json(swap_input(150, probes));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["true_ids"], json!([]), "{parsed}");
        assert_eq!(parsed["data"]["error_ids"], json!(["c0"]), "{parsed}");
    }

    #[test]
    fn reconciliation_matches_real_verdict() {
        // Binds the dual-materialization invariant: the verdict path and the
        // probe path must see ONE materialized context. We build a single `base`
        // input — the swap fixture at slippage 150 + the SHIPPED
        // `high-slippage-warning` forbid (`when { context.slippageBp > 100 }`,
        // @severity warn) — and feed it to BOTH exports. If the two
        // materialization paths (evaluate inlines its own; diagnosis goes through
        // `materialized_context`) ever diverge, the assertions below disagree.
        let (body, meta) = crate::action_eval_exports::tests::swap_sample_with_slippage(150);
        let base = json!({
            "action": body,
            "meta": meta,
            "tx": { "chain_id": "eip155:42161",
                    "from": "0x1111111111111111111111111111111111111111",
                    "to":   "0x2222222222222222222222222222222222222222" },
            "bundles": [ crate::action_eval_exports::tests::shipped_high_slippage_bundle() ],
            "results": {}
        });

        // 1. REAL verdict: the shipped forbid fires (severity warn → kind warn).
        let eval_out = crate::action_eval_exports::evaluate_action_v2_json(base.to_string());
        let eval_parsed: Value = serde_json::from_str(&eval_out).unwrap();
        assert_eq!(
            eval_parsed["data"]["verdict"]["kind"], "warn",
            "shipped high-slippage forbid must fire at slippage 150: {eval_parsed}"
        );

        // 2. PROBE the forbid's single `when` body against the SAME input. It must
        //    be TRUE — the diagnosis agrees the forbid fired on this context.
        let mut diag = base;
        diag["probes"] = json!([
            { "id": "c0.body", "est": {
                "effect": "permit",
                "principal": { "op": "All" }, "action": { "op": "All" }, "resource": { "op": "All" },
                "conditions": [{ "kind": "when", "body":
                    { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "slippageBp" } },
                             "right": { "Value": 100 } } } }] } }
        ]);
        let out = run_diagnosis_probes_v2_json(diag.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            parsed["data"]["true_ids"],
            json!(["c0.body"]),
            "the when-body the forbid fired on must probe TRUE: {parsed}"
        );
    }

    #[test]
    fn accepts_real_blockstoest_shaped_probe_est() {
        // This EST is byte-shaped exactly as the TS `blocksToEst(probePolicy(...))`
        // emits: unconstrained permit (`{op:"All"}` scopes) + an `annotations` OBJECT
        // carrying @id. The seam Task 16's manual click would otherwise be first to
        // hit — verified here against Policy::from_json (cedar 4.10).
        let probes = json!([
            { "id": "c0.body", "est": {
                "effect": "permit",
                "principal": { "op": "All" },
                "action": { "op": "All" },
                "resource": { "op": "All" },
                "annotations": { "id": "c0.body" },
                "conditions": [{ "kind": "when", "body":
                    { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "slippageBp" } },
                             "right": { "Value": 100 } } } }] } }
        ]);
        let out = run_diagnosis_probes_v2_json(swap_input(150, probes));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            parsed["ok"], true,
            "annotated unconstrained-permit EST must parse: {parsed}"
        );
        assert_eq!(parsed["data"]["true_ids"], json!(["c0.body"]), "{parsed}");
    }

    #[test]
    fn empty_probes_yields_empty_lists_no_panic() {
        let out = run_diagnosis_probes_v2_json(swap_input(150, json!([])));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["true_ids"], json!([]), "{parsed}");
        assert_eq!(parsed["data"]["error_ids"], json!([]), "{parsed}");
    }

    #[test]
    fn duplicate_probe_ids_error_on_add() {
        // Two valid permits sharing one id: the second `PolicySet::add` rejects
        // the duplicate, surfacing the `probe_add` error envelope.
        let probe = |id: &str| {
            json!({ "id": id, "est": {
            "effect": "permit",
            "principal": { "op": "All" }, "action": { "op": "All" }, "resource": { "op": "All" },
            "conditions": [{ "kind": "when", "body":
                { ">": { "left": { ".": { "left": { "Var": "context" }, "attr": "slippageBp" } },
                         "right": { "Value": 100 } } } }] } })
        };
        let probes = json!([probe("dup"), probe("dup")]);
        let out = run_diagnosis_probes_v2_json(swap_input(150, probes));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "probe_add", "{parsed}");
    }
}
