//! Denial diagnosis: Cedar-probe oracle. See
//! docs/superpowers/specs/2026-06-05-blockir-denial-diagnosis-design.md.

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
