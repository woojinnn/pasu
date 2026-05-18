//! Manifest-driven policy RPC planning and context materialization.

mod manifest;
mod materialize;
mod planning;
mod selector;

pub use manifest::{
    validate_manifests, ContextProjection, PolicyManifest, PolicyRpcCall, PolicyRpcError,
    PolicyRpcErrorBody, PolicyRpcResponse, PolicyRpcResult, ProjectionType, Requirement,
    RequirementWhen, RootInput,
};
pub use materialize::{
    apply_rpc_results, apply_rpc_results_with_indices, system_fail_verdict, SYSTEM_POLICY_ID,
};
pub use planning::{manifest_set_hash, plan_calls};
pub use selector::resolve_selector;

#[cfg(test)]
mod tests {
    use crate::action::{Action, ActionEnvelope};
    use serde_json::{json, Value};

    fn manifest_json(required: bool) -> Value {
        json!({
            "id": "user/max-input-usd-100",
            "schema_version": 1,
            "requires": [{
                "id": "swap-total-input-usd",
                "when": { "action": "swap" },
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id",
                    "asset": "$.action.inputToken.asset",
                    "amount": "$.action.inputToken.amount.value"
                },
                "outputs": [{
                    "kind": "context",
                    "field": "totalInputUsd",
                    "type": "UsdValuation",
                    "from": "$.result",
                    "required": required
                }]
            }],
            "context_extensions": {
                "swap": { "totalInputUsd": "UsdValuation" }
            }
        })
    }

    fn swap_envelope() -> ActionEnvelope {
        serde_json::from_value(json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "symbol": "WETH",
                        "decimals": 18
                    },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "outputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6
                    },
                    "amount": { "kind": "min", "value": "900000000" }
                },
                "recipient": "0x1111111111111111111111111111111111111111"
            }
        }))
        .unwrap()
    }

    #[test]
    fn manifest_plans_swap_oracle_call_from_selectors() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(true))
            .expect("manifest parses");
        let root = super::RootInput {
            chain_id: 1,
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            to: "0x2222222222222222222222222222222222222222".to_owned(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(1_700_000_000),
        };

        let calls = super::plan_calls(&root, &[swap_envelope()], &[manifest], &json!({})).unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].id,
            "user/max-input-usd-100::0::swap-total-input-usd"
        );
        assert_eq!(calls[0].method, "oracle.usd_value");
        assert_eq!(
            calls[0].params,
            json!({
                "chain_id": 1,
                "asset": {
                    "kind": "erc20",
                    "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    "symbol": "WETH",
                    "decimals": 18
                },
                "amount": "1000000000000000000"
            })
        );
    }

    #[test]
    fn planned_call_ids_are_unique_per_manifest_and_action_index() {
        let mut manifest_a = manifest_json(false);
        manifest_a["id"] = json!("user/a");
        manifest_a["requires"][0]["id"] = json!("quote");
        let mut manifest_b = manifest_json(false);
        manifest_b["id"] = json!("user/b");
        manifest_b["requires"][0]["id"] = json!("quote");
        let manifests = vec![
            serde_json::from_value::<super::PolicyManifest>(manifest_a).unwrap(),
            serde_json::from_value::<super::PolicyManifest>(manifest_b).unwrap(),
        ];
        let root = super::RootInput {
            chain_id: 1,
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            to: "0x2222222222222222222222222222222222222222".to_owned(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(1_700_000_000),
        };

        let calls = super::plan_calls(
            &root,
            &[swap_envelope(), swap_envelope()],
            &manifests,
            &json!({}),
        )
        .unwrap();

        let ids = calls
            .iter()
            .map(|call| call.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), calls.len());
        assert!(ids.contains("user/a::0::quote"));
        assert!(ids.contains("user/a::1::quote"));
        assert!(ids.contains("user/b::0::quote"));
        assert!(ids.contains("user/b::1::quote"));
    }

    #[test]
    fn planning_selectors_can_read_lowered_base_context() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/context-selector",
            "schema_version": 1,
            "requires": [{
                "id": "swap-recipient",
                "when": { "action": "swap" },
                "method": "debug.echo",
                "params": {
                    "recipient": "$.context.recipient"
                },
                "outputs": []
            }],
            "context_extensions": {}
        }))
        .expect("manifest parses");
        let root = super::RootInput {
            chain_id: 1,
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            to: "0x2222222222222222222222222222222222222222".to_owned(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(1_700_000_000),
        };

        let calls = super::plan_calls(&root, &[swap_envelope()], &[manifest], &json!({})).unwrap();

        assert_eq!(
            calls[0].params,
            json!({ "recipient": "0x1111111111111111111111111111111111111111" })
        );
    }

    #[test]
    fn selector_rejects_arrays_and_wildcards() {
        let action = serde_json::to_value(match swap_envelope().action {
            Action::Swap(action) => action,
            _ => unreachable!("fixture is swap"),
        })
        .unwrap();
        let root = json!({ "chain_id": 1 });

        assert!(super::resolve_selector(
            "$.action.inputs[0].asset",
            &root,
            &action,
            &json!({}),
            &json!({}),
            &json!({})
        )
        .is_err());
        assert!(super::resolve_selector(
            "$.action.inputs[*]",
            &root,
            &action,
            &json!({}),
            &json!({}),
            &json!({})
        )
        .is_err());
    }

    #[test]
    fn materialization_inserts_required_usd_valuation_context() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(true))
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/max-input-usd-100::0::swap-total-input-usd".to_owned(),
                    ok: true,
                    result: Some(json!({
                        "value": "3500.1200",
                        "asOfTs": 1_700_000_000,
                        "staleSec": 5,
                        "sources": ["coingecko"]
                    })),
                    error: None,
                }],
            },
        )
        .unwrap();

        assert_eq!(
            requests[0].context["custom"]["totalInputUsd"],
            json!({
                "value": { "__extn": { "fn": "decimal", "arg": "3500.1200" } },
                "asOfTs": 1_700_000_000,
                "staleSec": 5,
                "sources": ["coingecko"]
            })
        );
    }

    #[test]
    fn duplicate_requirement_ids_are_rejected() {
        let mut manifest_json = manifest_json(false);
        let duplicate = manifest_json["requires"][0].clone();
        manifest_json["requires"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json)
            .expect("manifest parses");
        let root = super::RootInput {
            chain_id: 1,
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            to: "0x2222222222222222222222222222222222222222".to_owned(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(1_700_000_000),
        };

        let error =
            super::plan_calls(&root, &[swap_envelope()], &[manifest], &json!({})).unwrap_err();

        assert!(error
            .to_string()
            .contains("duplicate requirement id `swap-total-input-usd`"));
    }

    #[test]
    fn optional_projection_type_error_omits_context_field() {
        // D9: when requirement.optional=true, a type coercion failure on the
        // projected payload is swallowed and the field is simply omitted from
        // context.custom — evaluation continues.
        let mut manifest_value = manifest_json(false);
        manifest_value["requires"][0]["optional"] = json!(true);
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_value)
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/max-input-usd-100::0::swap-total-input-usd".to_owned(),
                    ok: true,
                    result: Some(json!({ "value": 3500 })),
                    error: None,
                }],
            },
        )
        .unwrap();

        // No top-level field and no nested custom field for totalInputUsd.
        assert!(requests[0].context.get("totalInputUsd").is_none());
        let custom = requests[0].context.get("custom");
        assert!(
            custom.is_none() || custom.and_then(|c| c.get("totalInputUsd")).is_none(),
            "expected totalInputUsd omitted from context.custom, got {custom:?}"
        );
    }

    #[test]
    fn materialization_rejects_duplicate_and_extra_response_ids() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(true))
            .expect("manifest parses");
        let envelope = swap_envelope();
        let request = crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers");

        let valid_result = super::PolicyRpcResult {
            id: "user/max-input-usd-100::0::swap-total-input-usd".to_owned(),
            ok: true,
            result: Some(json!({
                "value": "3500.1200",
                "asOfTs": 1_700_000_000,
                "staleSec": 5,
                "sources": ["coingecko"]
            })),
            error: None,
        };

        let mut duplicate_requests = vec![request.clone()];
        let duplicate_error = super::apply_rpc_results(
            &mut duplicate_requests,
            std::slice::from_ref(&envelope),
            std::slice::from_ref(&manifest),
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![valid_result.clone(), valid_result.clone()],
            },
        )
        .unwrap_err();
        assert!(duplicate_error.to_string().contains("duplicate result id"));

        let mut extra_requests = vec![request];
        let extra_error = super::apply_rpc_results(
            &mut extra_requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![
                    valid_result,
                    super::PolicyRpcResult {
                        id: "unexpected".to_owned(),
                        ok: true,
                        result: Some(json!({})),
                        error: None,
                    },
                ],
            },
        )
        .unwrap_err();
        assert!(extra_error.to_string().contains("unexpected result id"));
    }

    #[test]
    fn materialization_rejects_projection_over_existing_context_field() {
        // D3: outputs live under `context.custom.<field>`. Two manifests
        // declaring the same custom field collide.
        let manifest_a = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/custom-a",
            "schema_version": 1,
            "requires": [{
                "id": "produce-foo",
                "when": { "action": "swap" },
                "method": "debug.echo",
                "params": {},
                "outputs": [{
                    "kind": "context",
                    "field": "foo",
                    "type": "String",
                    "from": "$.result.foo",
                    "required": true
                }]
            }],
            "context_extensions": {
                "swap": { "foo": "String" }
            }
        }))
        .expect("manifest parses");
        let manifest_b = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/custom-b",
            "schema_version": 1,
            "requires": [{
                "id": "produce-foo-again",
                "when": { "action": "swap" },
                "method": "debug.echo",
                "params": {},
                "outputs": [{
                    "kind": "context",
                    "field": "foo",
                    "type": "String",
                    "from": "$.result.foo",
                    "required": true
                }]
            }],
            "context_extensions": {
                "swap": { "foo": "String" }
            }
        }))
        .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        let error = super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest_a, manifest_b],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![
                    super::PolicyRpcResult {
                        id: "user/custom-a::0::produce-foo".to_owned(),
                        ok: true,
                        result: Some(json!({ "foo": "alpha" })),
                        error: None,
                    },
                    super::PolicyRpcResult {
                        id: "user/custom-b::0::produce-foo-again".to_owned(),
                        ok: true,
                        result: Some(json!({ "foo": "beta" })),
                        error: None,
                    },
                ],
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("would overwrite an existing context field"),
            "{error}"
        );
    }

    #[test]
    fn params_selector_uses_supplied_params_root() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/params-root",
            "schema_version": 1,
            "requires": [{
                "id": "swap-with-origin",
                "when": { "action": "swap" },
                "method": "debug.echo",
                "params": {
                    "origin": "$.params.origin"
                },
                "outputs": []
            }],
            "context_extensions": {}
        }))
        .expect("manifest parses");
        let root = super::RootInput {
            chain_id: 1,
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            to: "0x2222222222222222222222222222222222222222".to_owned(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(1_700_000_000),
        };

        let calls = super::plan_calls(
            &root,
            &[swap_envelope()],
            &[manifest],
            &json!({ "origin": "wallet-ui" }),
        )
        .unwrap();

        assert_eq!(calls[0].params, json!({ "origin": "wallet-ui" }));
    }

    #[test]
    fn generated_schema_accepts_duplicate_same_type_and_rejects_conflict() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(false))
            .expect("manifest parses");
        let preview = crate::schema::PolicySchemaComposer::new()
            .with_manifests(std::slice::from_ref(&manifest))
            .unwrap()
            .preview();

        // Post-Phase-2 the base no longer ships totalInputUsd, so the legacy
        // composer adds it from the manifest's context_extensions block.
        assert!(preview.schema_text.contains("totalInputUsd?: UsdValuation"));
        assert!(preview
            .added_fields
            .iter()
            .any(|field| field.field == "totalInputUsd"));

        let conflict = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/conflict",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "totalInputUsd": "Long" }
            }
        }))
        .unwrap();

        assert!(crate::schema::PolicySchemaComposer::new()
            .with_manifests(&[manifest, conflict])
            .is_err());
    }

    #[test]
    fn generated_schema_accepts_base_field_same_type_and_rejects_conflict() {
        let same_type = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/base-same-type",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "recipient": "String" }
            }
        }))
        .unwrap();

        let preview = crate::schema::PolicySchemaComposer::new()
            .with_manifests(&[same_type])
            .unwrap()
            .preview();
        assert!(preview
            .added_fields
            .iter()
            .all(|field| field.field != "recipient"));

        let conflict = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/base-collision",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "recipient": "Long" }
            }
        }))
        .unwrap();

        assert!(crate::schema::PolicySchemaComposer::new()
            .with_manifests(&[conflict])
            .is_err());
    }

    #[test]
    fn materialization_inserts_window_stats_context() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/window-stats",
            "schema_version": 1,
            "requires": [{
                "id": "swap-window-stats",
                "when": { "action": "swap" },
                "method": "stat_window.swap_stats",
                "params": {},
                "outputs": [{
                    "kind": "context",
                    "field": "windowStats",
                    "type": "WindowStats",
                    "from": "$.result",
                    "required": true
                }]
            }],
            "context_extensions": {
                "swap": { "windowStats": "WindowStats" }
            }
        }))
        .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/window-stats::0::swap-window-stats".to_owned(),
                    ok: true,
                    result: Some(json!({
                        "swapVolumeUsd24h": "42.0000",
                        "swapCount24h": 3
                    })),
                    error: None,
                }],
            },
        )
        .unwrap();

        assert_eq!(
            requests[0].context["custom"]["windowStats"],
            json!({
                "swapVolumeUsd24h": { "__extn": { "fn": "decimal", "arg": "42.0000" } },
                "swapCount24h": 3
            })
        );
    }

    #[test]
    fn schema_swap_extension_manifest_plans_legacy_enrichment_calls() {
        // Post-Phase-2 the shipped swap manifest no longer hand-declares
        // `context_extensions` — the composer derives them from outputs. The
        // test now exercises the planning path to assert the manifest still
        // emits the same set of RPC method calls plus the two new ones moved
        // from base into manifest-driven enrichment.
        let manifest = serde_json::from_str::<super::PolicyManifest>(include_str!(
            "../../../../schema/policy-schema/extensions/DEX/swap.policy-rpc.json"
        ))
        .expect("schema extension manifest parses");
        let root = super::RootInput {
            chain_id: 1,
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            to: "0x2222222222222222222222222222222222222222".to_owned(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(1_700_000_000),
        };

        assert!(
            manifest.context_extensions.is_empty(),
            "context_extensions must be derived, not hand-authored: {:?}",
            manifest.context_extensions
        );

        let output_fields = manifest
            .requires
            .iter()
            .flat_map(|req| req.outputs.iter().map(|out| out.field.as_str()))
            .collect::<std::collections::BTreeSet<_>>();
        for expected in [
            "totalInputUsd",
            "totalMinOutputUsd",
            "effectiveRateVsOracleBps",
            "totalInputFractionOfPortfolioBps",
            "windowStats",
            "validityDeltaSec",
            "recipientIsContract",
        ] {
            assert!(
                output_fields.contains(expected),
                "swap manifest must still produce `{expected}`"
            );
        }

        // Envelope with a validity block so the validity-delta-sec requirement
        // doesn't get skipped by the optional-param selector check.
        let envelope_with_validity: ActionEnvelope = serde_json::from_value(json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "symbol": "WETH",
                        "decimals": 18
                    },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "outputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6
                    },
                    "amount": { "kind": "min", "value": "900000000" }
                },
                "recipient": "0x1111111111111111111111111111111111111111",
                "validity": { "expiresAt": "1700000300", "source": "tx-deadline" }
            }
        }))
        .unwrap();
        let calls =
            super::plan_calls(&root, &[envelope_with_validity], &[manifest], &json!({})).unwrap();
        let methods = calls
            .iter()
            .map(|call| call.method.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(methods.contains("oracle.usd_value"));
        assert!(methods.contains("oracle.effective_rate_bps"));
        assert!(methods.contains("portfolio.input_fraction_bps"));
        assert!(methods.contains("stat_window.swap_stats"));
        assert!(methods.contains("clock.validity_delta_sec"));
        assert!(methods.contains("chain.is_contract"));
    }

    #[test]
    fn generated_schema_normalizes_decimal_alias() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/decimal-alias",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "tokenPrice": "Decimal" }
            }
        }))
        .unwrap();

        let preview = crate::schema::PolicySchemaComposer::new()
            .with_manifests(&[manifest])
            .unwrap()
            .preview();

        assert!(preview.schema_text.contains("tokenPrice?: decimal"));
        assert_eq!(preview.added_fields[0].type_name, "decimal");
    }

    // -----------------------------------------------------------------
    // D9 — runtime failure model (Phase 3)
    // -----------------------------------------------------------------

    /// D9: a non-optional requirement whose RPC result returned ok=false
    /// produces `PolicyRpcError::SystemFail`. The caller boundary maps that
    /// into a synthetic `Verdict::Fail` with `policy_id == "__system__"` and
    /// reason starting with "rpc-unavailable: ".
    #[test]
    fn rpc_failure_on_non_optional_requirement_produces_system_fail() {
        // optional=false (default) ⇒ ok:false materialization must SystemFail.
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(true))
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        let error = super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/max-input-usd-100::0::swap-total-input-usd".to_owned(),
                    ok: false,
                    result: None,
                    error: Some(super::PolicyRpcErrorBody {
                        code: "upstream".to_owned(),
                        message: "oracle offline".to_owned(),
                    }),
                }],
            },
        )
        .unwrap_err();

        match &error {
            super::PolicyRpcError::SystemFail { call_id, reason } => {
                assert_eq!(call_id, "user/max-input-usd-100::0::swap-total-input-usd");
                assert!(reason.contains("oracle offline"), "{reason}");
            }
            other => panic!("expected SystemFail, got {other:?}"),
        }

        // Verdict-shape assertion per the plan: `Verdict::Fail` with the
        // `__system__` matched policy and the required reason prefix.
        let verdict = super::system_fail_verdict(&error).expect("system fail produces a verdict");
        match verdict {
            crate::policy::Verdict::Fail(matched) => {
                assert!(
                    matched.iter().any(|p| p.policy_id == super::SYSTEM_POLICY_ID
                        && p.reason
                            .as_deref()
                            .unwrap_or("")
                            .starts_with("rpc-unavailable:")),
                    "expected __system__ matched policy with rpc-unavailable reason, got {matched:?}"
                );
            }
            other => panic!("expected Verdict::Fail, got {other:?}"),
        }
    }

    /// D9: when `requirement.optional == true`, an ok=false RPC result is
    /// swallowed silently — the projected field is omitted from
    /// `context.custom.*` and evaluation continues.
    #[test]
    fn rpc_failure_on_optional_requirement_omits_field_and_continues() {
        let mut manifest_value = manifest_json(true);
        manifest_value["requires"][0]["optional"] = json!(true);
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_value)
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/max-input-usd-100::0::swap-total-input-usd".to_owned(),
                    ok: false,
                    result: None,
                    error: Some(super::PolicyRpcErrorBody {
                        code: "upstream".to_owned(),
                        message: "oracle offline".to_owned(),
                    }),
                }],
            },
        )
        .expect("optional requirement absorbs ok=false");

        // No top-level field, and either no `custom` block or no totalInputUsd
        // inside it. The contract is "field omitted", not "custom not created".
        assert!(requests[0].context.get("totalInputUsd").is_none());
        let custom = requests[0].context.get("custom");
        assert!(
            custom.is_none() || custom.and_then(|c| c.get("totalInputUsd")).is_none(),
            "expected totalInputUsd to be omitted from context.custom, got {custom:?}"
        );
    }

    /// D9: a non-optional requirement whose payload fails per-field type
    /// coercion also produces `SystemFail` (not the legacy `RpcResult` error).
    #[test]
    fn rpc_type_coercion_failure_on_non_optional_requirement_produces_system_fail() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(true))
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        let error = super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/max-input-usd-100::0::swap-total-input-usd".to_owned(),
                    ok: true,
                    // value must be string per UsdValuation, but is a Number.
                    result: Some(json!({ "value": 3500 })),
                    error: None,
                }],
            },
        )
        .unwrap_err();

        assert!(
            matches!(error, super::PolicyRpcError::SystemFail { .. }),
            "{error:?}"
        );
        assert!(super::system_fail_verdict(&error).is_some());
    }

    /// C: when `manifest.context_extensions` is empty, the materializer must
    /// derive the declaration set from `requires[].outputs[]` rather than
    /// rejecting the projection as `undeclared`. The shipped manifests now
    /// arrive with empty `context_extensions` per Phase 2.
    #[test]
    fn materialization_accepts_derived_declarations_when_context_extensions_empty() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/derive-only",
            "schema_version": 1,
            "requires": [{
                "id": "swap-total-input-usd",
                "when": { "action": "swap" },
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id",
                    "asset": "$.action.inputToken.asset",
                    "amount": "$.action.inputToken.amount.value"
                },
                "outputs": [{
                    "kind": "context",
                    "field": "totalInputUsd",
                    "type": "UsdValuation",
                    "from": "$.result",
                    "required": true
                }]
            }],
            "context_extensions": {}
        }))
        .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/derive-only::0::swap-total-input-usd".to_owned(),
                    ok: true,
                    result: Some(json!({
                        "value": "1234.5678",
                        "asOfTs": 1_700_000_000_u64,
                        "staleSec": 5,
                        "sources": ["coingecko"]
                    })),
                    error: None,
                }],
            },
        )
        .expect("derived declarations should validate without context_extensions");

        let custom = requests[0]
            .context
            .get("custom")
            .and_then(|c| c.get("totalInputUsd"))
            .expect("context.custom.totalInputUsd materialized");
        assert_eq!(
            custom["value"],
            json!({ "__extn": { "fn": "decimal", "arg": "1234.5678" } })
        );
    }

    /// C: backwards-compat — when `context_extensions` is non-empty the legacy
    /// validator still fires. A projection that the legacy block does not
    /// declare must still surface `InvalidManifest("undeclared …")`.
    #[test]
    fn materialization_rejects_undeclared_projection_with_legacy_context_extensions() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/legacy-strict",
            "schema_version": 1,
            "requires": [{
                "id": "swap-total-input-usd",
                "when": { "action": "swap" },
                "method": "oracle.usd_value",
                "params": {},
                "outputs": [{
                    "kind": "context",
                    "field": "totalInputUsd",
                    "type": "UsdValuation",
                    "from": "$.result",
                    "required": true
                }]
            }],
            // Legacy block declares an unrelated field but NOT totalInputUsd.
            "context_extensions": {
                "swap": { "tokenRiskScore": "Long" }
            }
        }))
        .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        let error = super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![super::PolicyRpcResult {
                    id: "user/legacy-strict::0::swap-total-input-usd".to_owned(),
                    ok: true,
                    result: Some(json!({
                        "value": "1.0000",
                        "asOfTs": 1_700_000_000_u64,
                        "staleSec": 5,
                        "sources": ["coingecko"]
                    })),
                    error: None,
                }],
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                super::PolicyRpcError::InvalidManifest(ref msg) if msg.contains("undeclared")
            ),
            "{error:?}"
        );
    }

    /// D9: a missing RPC result for a non-optional requirement must route
    /// through the same D9 branch as ok=false (i.e. `SystemFail`), not the
    /// legacy generic `RpcResult("missing result …")` early return.
    #[test]
    fn missing_rpc_result_on_non_optional_requirement_produces_system_fail() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(true))
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        // No matching result entry at all — the call id the materializer
        // expects is `user/max-input-usd-100::0::swap-total-input-usd`, but
        // the response carries an empty result vec.
        let error = super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![],
            },
        )
        .unwrap_err();

        match &error {
            super::PolicyRpcError::SystemFail { call_id, reason } => {
                assert_eq!(call_id, "user/max-input-usd-100::0::swap-total-input-usd");
                assert!(
                    reason.starts_with("missing"),
                    "expected reason to start with `missing`, got `{reason}`"
                );
            }
            other => panic!("expected SystemFail, got {other:?}"),
        }
        assert!(super::system_fail_verdict(&error).is_some());
    }

    /// D9: a missing RPC result for an optional requirement is swallowed; the
    /// projected field is omitted from `context.custom` and the call succeeds.
    #[test]
    fn missing_rpc_result_on_optional_requirement_omits_field_and_continues() {
        let mut manifest_value = manifest_json(true);
        manifest_value["requires"][0]["optional"] = json!(true);
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_value)
            .expect("manifest parses");
        let envelope = swap_envelope();
        let mut requests = vec![crate::policy_request_from_envelope(
            &envelope,
            &"0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            &"0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            &"0".parse().unwrap(),
            1,
            1_700_000_000,
        )
        .expect("swap lowers")];

        super::apply_rpc_results(
            &mut requests,
            &[envelope],
            &[manifest],
            &super::PolicyRpcResponse {
                request_id: "eval-1".to_owned(),
                results: vec![],
            },
        )
        .expect("optional requirement absorbs missing result");

        assert!(requests[0].context.get("totalInputUsd").is_none());
        let custom = requests[0].context.get("custom");
        assert!(
            custom.is_none() || custom.and_then(|c| c.get("totalInputUsd")).is_none(),
            "expected totalInputUsd to be omitted from context.custom, got {custom:?}"
        );
    }
}
