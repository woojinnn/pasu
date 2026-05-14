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
pub use materialize::{apply_rpc_results, apply_rpc_results_with_indices};
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
                    "asset": "$.action.tokenIn",
                    "amount": "$.action.amountIn.value"
                },
                "outputs": [{
                    "kind": "context",
                    "field": "rpcTotalInputUsd",
                    "type": "UsdValuation",
                    "from": "$.result",
                    "required": required
                }]
            }],
            "context_extensions": {
                "swap": { "rpcTotalInputUsd": "UsdValuation" }
            }
        })
    }

    fn swap_envelope() -> ActionEnvelope {
        serde_json::from_value(json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "tokenIn": {
                    "kind": "erc20",
                    "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    "symbol": "WETH",
                    "decimals": 18
                },
                "tokenOut": {
                    "kind": "erc20",
                    "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                    "symbol": "USDC",
                    "decimals": 6
                },
                "amountIn": { "kind": "exact", "value": "1000000000000000000" },
                "amountOut": { "kind": "min", "value": "900000000" },
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
            requests[0].context["rpcTotalInputUsd"],
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
        let manifest = serde_json::from_value::<super::PolicyManifest>(manifest_json(false))
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

        assert!(requests[0].context.get("rpcTotalInputUsd").is_none());
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
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/context-overwrite",
            "schema_version": 1,
            "requires": [{
                "id": "overwrite-recipient",
                "when": { "action": "swap" },
                "method": "debug.echo",
                "params": {},
                "outputs": [{
                    "kind": "context",
                    "field": "recipient",
                    "type": "String",
                    "from": "$.result.recipient",
                    "required": true
                }]
            }],
            "context_extensions": {
                "swap": { "recipient": "String" }
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
                    id: "user/context-overwrite::0::overwrite-recipient".to_owned(),
                    ok: true,
                    result: Some(json!({
                        "recipient": "0x9999999999999999999999999999999999999999"
                    })),
                    error: None,
                }],
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("would overwrite an existing context field"),
            "{error}"
        );
        assert_eq!(
            requests[0].context["recipient"],
            json!("0x1111111111111111111111111111111111111111")
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

        assert!(preview
            .schema_text
            .contains("rpcTotalInputUsd?: UsdValuation"));

        let conflict = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/conflict",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "rpcTotalInputUsd": "Long" }
            }
        }))
        .unwrap();

        assert!(crate::schema::PolicySchemaComposer::new()
            .with_manifests(&[manifest, conflict])
            .is_err());
    }

    #[test]
    fn generated_schema_rejects_base_field_collision() {
        let manifest = serde_json::from_value::<super::PolicyManifest>(json!({
            "id": "user/base-collision",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "recipient": "String" }
            }
        }))
        .unwrap();

        assert!(crate::schema::PolicySchemaComposer::new()
            .with_manifests(&[manifest])
            .is_err());
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
}
