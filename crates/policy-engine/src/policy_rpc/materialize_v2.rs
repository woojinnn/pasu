//! v2 policy-rpc result materialization into the lowered action context.
//!
//! Additive counterpart to v1 [`super::materialize::apply_rpc_results`]. The
//! shape differs in two deliberate ways that change the fail-open surface:
//!
//! 1. **Results are raw values.** The host hands back a `call_id -> Value` map
//!    (the unwrapped `$.result` payload), not the v1 `PolicyRpcResult { ok,
//!    error, result }` envelope. There is therefore no `ok == false` branch to
//!    mirror — a failed call is simply *absent* from the map, which collapses
//!    onto v1's `None` (missing-result) branch.
//! 2. **Context is the lowered Cedar JSON.** Projected fields are written under
//!    `context["custom"][<field>]`, which the swap (and every) lowering
//!    deliberately omits — so the `custom` slot is allocated on first use, then
//!    guarded against cross-call collisions exactly as v1's `insert_custom_field`.
//!
//! Fail-open semantics, mirrored from v1's `d9_branch` + per-output branch:
//! - missing result, `optional == false` → [`PolicyRpcError::SystemFail`];
//! - missing result, `optional == true`  → skip every output silently;
//! - per-output projection/coercion failure, `optional == false` → `SystemFail`;
//! - per-output projection/coercion failure, `optional == true`  → continue.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::planning_v2::PlannedCallV2;
use super::{PolicyRpcError, ProjectionType};

/// Apply raw policy-rpc results into the lowered action `context`'s `custom`.
///
/// For each [`PlannedCallV2`], look its result up in `results` by `call_id`.
/// Missing-result handling follows the fail-open contract in the module docs.
/// For a present result, every [`ContextProjection`] output resolves its
/// `$.result` selector against the raw payload, coerces the selected value to
/// the projection's declared Cedar shape, and writes it to
/// `context["custom"][output.field]`.
///
/// # Errors
///
/// Returns [`PolicyRpcError::SystemFail`] when a *required* call is missing from
/// `results` or its required projection fails (selector miss / type-coercion
/// failure). Returns [`PolicyRpcError::RpcResult`] when `context` (or its
/// `custom` slot) is not a JSON object, or when two calls project the same
/// `custom` field. Returns [`PolicyRpcError::InvalidManifest`] for an
/// unsupported projection `kind`.
pub fn materialize_v2(
    context: &mut Value,
    planned: &[PlannedCallV2],
    results: &BTreeMap<String, Value>,
) -> Result<(), PolicyRpcError> {
    for call in planned {
        let Some(payload) = results.get(&call.call_id) else {
            // Mirror v1's `None` branch: optional → skip, required → SystemFail.
            if call.optional {
                continue;
            }
            return Err(PolicyRpcError::SystemFail {
                call_id: call.call_id.clone(),
                reason: "missing rpc result".to_owned(),
            });
        };
        apply_call_outputs(context, call, payload)?;
    }
    Ok(())
}

fn apply_call_outputs(
    context: &mut Value,
    call: &PlannedCallV2,
    payload: &Value,
) -> Result<(), PolicyRpcError> {
    let empty = Value::Object(Map::new());
    for output in &call.outputs {
        if output.kind != "context" {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "unsupported projection kind `{}`",
                output.kind
            )));
        }
        let materialized =
            match super::resolve_selector(&output.from, &empty, &empty, &empty, payload, &empty)
                .and_then(|selected| materialize_value(&selected, &output.type_name))
            {
                Ok(value) => value,
                Err(error) => {
                    // Mirror v1's per-output branch: branch on `optional`, not on
                    // the legacy `output.required` discriminator.
                    if call.optional {
                        continue;
                    }
                    return Err(PolicyRpcError::SystemFail {
                        call_id: call.call_id.clone(),
                        reason: error.to_string(),
                    });
                }
            };
        insert_custom_field(context, &output.field, materialized)?;
    }
    Ok(())
}

/// Write `value` under `context.custom.<field>`, allocating the `custom` record
/// on first use. Mirrors v1's `insert_custom_field`: the swap lowering omits
/// `custom`, and two calls projecting the same field is a collision error.
fn insert_custom_field(
    context: &mut Value,
    field: &str,
    value: Value,
) -> Result<(), PolicyRpcError> {
    let object = context.as_object_mut().ok_or_else(|| {
        PolicyRpcError::RpcResult("lowered action context is not an object".to_owned())
    })?;
    let custom = object
        .entry("custom")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| PolicyRpcError::RpcResult("context.custom is not an object".to_owned()))?;
    if custom.contains_key(field) {
        return Err(PolicyRpcError::InvalidManifest(format!(
            "context projection `custom.{field}` would overwrite an existing context field"
        )));
    }
    custom.insert(field.to_owned(), value);
    Ok(())
}

/// Coerce a raw selected value to its declared Cedar projection shape.
///
/// This re-implements the subset of v1's private `materialize::materialize_value`
/// that the v2 manifests need, using the public [`crate::cedar_json`] helpers so
/// a `decimal` projection becomes the `{"__extn":{"fn":"decimal","arg":…}}` form
/// that strict `Context::from_json_value` validation requires. The removed
/// legacy record types (`UsdValuation` / `WindowStats`) are intentionally not
/// supported in v2 — `custom_context` declares only Cedar primitives / `decimal`
/// / `Set<…>`.
fn materialize_value(value: &Value, type_name: &ProjectionType) -> Result<Value, PolicyRpcError> {
    match type_name {
        ProjectionType::String => value
            .as_str()
            .map(Value::from)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected String".to_owned())),
        ProjectionType::Long => value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
            .map(Value::from)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected Long".to_owned())),
        ProjectionType::Bool => value
            .as_bool()
            .map(Value::from)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected Bool".to_owned())),
        ProjectionType::Decimal => value
            .as_str()
            .map(crate::cedar_json::decimal_json)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected Decimal string".to_owned())),
        ProjectionType::SetString => {
            let array = value.as_array().ok_or_else(|| {
                PolicyRpcError::RpcResult("expected Set<String> array".to_owned())
            })?;
            let mut out = Vec::with_capacity(array.len());
            for entry in array {
                let entry = entry.as_str().ok_or_else(|| {
                    PolicyRpcError::RpcResult("expected Set<String> entry string".to_owned())
                })?;
                out.push(Value::from(entry));
            }
            Ok(Value::Array(out))
        }
        // v2 manifests declare only the types `compose_per_policy` injects;
        // the legacy record types are not produced by the new model.
        ProjectionType::UsdValuation | ProjectionType::WindowStats => {
            Err(PolicyRpcError::RpcResult(format!(
                "projection type `{}` is not supported in v2 materialization",
                type_name.cedar_type()
            )))
        }
    }
}

#[cfg(test)]
// The `swap_sample` fixture is a verbatim, deliberately literal mirror of
// `amm::swap`'s reference sample (a faithful UniswapV3 swap), so the same allow
// set the reference test module carries applies here.
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use super::*;
    use crate::policy_rpc::{plan_policy_rpc_v2, ContextProjection, ManifestV2, TxView};
    use serde_json::json;

    fn projection(field: &str, type_name: &str, from: &str) -> ContextProjection {
        serde_json::from_value(json!({
            "kind": "context", "field": field, "type": type_name, "from": from
        }))
        .unwrap()
    }

    fn planned(call_id: &str, outputs: Vec<ContextProjection>, optional: bool) -> PlannedCallV2 {
        PlannedCallV2 {
            manifest_id: "m".to_owned(),
            call_id: call_id.to_owned(),
            method: "oracle.usd_value".to_owned(),
            params: json!({}),
            outputs,
            optional,
        }
    }

    #[test]
    fn writes_decimal_projection_into_custom() {
        let mut context = json!({ "slippageBp": 50 });
        let calls = vec![planned(
            "m::c",
            vec![projection("totalInputUsd", "Decimal", "$.result.usd")],
            false,
        )];
        let mut results = BTreeMap::new();
        results.insert("m::c".to_owned(), json!({ "usd": "3500.1200" }));

        materialize_v2(&mut context, &calls, &results).unwrap();

        assert_eq!(
            context["custom"]["totalInputUsd"],
            json!({ "__extn": { "fn": "decimal", "arg": "3500.1200" } })
        );
        // Untouched base field survives.
        assert_eq!(context["slippageBp"], json!(50));
    }

    #[test]
    fn missing_required_result_system_fails_optional_skips() {
        let mut context = json!({});
        let required = vec![planned(
            "m::c",
            vec![projection("x", "String", "$.result.x")],
            false,
        )];
        let err = materialize_v2(&mut context, &required, &BTreeMap::new()).unwrap_err();
        match &err {
            PolicyRpcError::SystemFail { call_id, reason } => {
                assert_eq!(call_id, "m::c");
                assert!(reason.starts_with("missing"), "{reason}");
            }
            other => panic!("expected SystemFail, got {other:?}"),
        }

        let mut context = json!({});
        let optional = vec![planned(
            "m::c",
            vec![projection("x", "String", "$.result.x")],
            true,
        )];
        materialize_v2(&mut context, &optional, &BTreeMap::new()).unwrap();
        assert!(context.get("custom").is_none());
    }

    #[test]
    fn required_projection_coercion_failure_system_fails() {
        // Declared Decimal but payload value is a number, not a string.
        let mut context = json!({});
        let calls = vec![planned(
            "m::c",
            vec![projection("totalInputUsd", "Decimal", "$.result.usd")],
            false,
        )];
        let mut results = BTreeMap::new();
        results.insert("m::c".to_owned(), json!({ "usd": 3500 }));

        let err = materialize_v2(&mut context, &calls, &results).unwrap_err();
        assert!(matches!(err, PolicyRpcError::SystemFail { .. }), "{err:?}");
    }

    #[test]
    fn duplicate_custom_field_across_calls_collides() {
        let mut context = json!({});
        let calls = vec![
            planned(
                "m::a",
                vec![projection("foo", "String", "$.result.foo")],
                false,
            ),
            planned(
                "m::b",
                vec![projection("foo", "String", "$.result.foo")],
                false,
            ),
        ];
        let mut results = BTreeMap::new();
        results.insert("m::a".to_owned(), json!({ "foo": "alpha" }));
        results.insert("m::b".to_owned(), json!({ "foo": "beta" }));

        let err = materialize_v2(&mut context, &calls, &results).unwrap_err();
        assert!(
            err.to_string()
                .contains("would overwrite an existing context field"),
            "{err}"
        );
    }

    // ---------------------------------------------------------------------
    // End-to-end: lower swap → plan → materialize → strict-validate context.
    // ---------------------------------------------------------------------

    fn swap_manifest() -> ManifestV2 {
        serde_json::from_value(json!({
            "id": "large-swap-usd-warning",
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": "swap" } } },
            "policy_rpc": [{
                "id": "total-input-usd",
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id",
                    "recipient": "$.action.recipient"
                },
                "outputs": [{
                    "kind": "context",
                    "field": "totalInputUsd",
                    "type": "Decimal",
                    "from": "$.result.usd"
                }]
            }],
            "custom_context": { "fields": { "totalInputUsd": "decimal" } }
        }))
        .expect("manifest parses")
    }

    /// Build the UniswapV3 swap sample (mirrors `amm::swap`'s test fixture, kept
    /// minimal). Returns the `(ActionBody, ActionMeta)` pair.
    fn swap_sample() -> (
        policy_transition::action::ActionBody,
        policy_transition::action::ActionMeta,
    ) {
        use std::str::FromStr;

        use policy_state::live_field::{DataSource, OracleProvider};
        use policy_state::primitives::{Address, ChainId, Duration, Time, U128, U256};
        use policy_state::token::{TokenKey, TokenRef};
        use policy_state::LiveField;
        use policy_transition::action::amm::{
            AmmAction, AmmVenue, PoolState, RouteHop, RoutePath, SwapAction, SwapDirection,
            SwapLiveInputs, SwapParams, SwapRoute,
        };
        use policy_transition::action::{ActionBody, ActionMeta, ActionNature};

        let now = Time::from_unix(1_738_000_000);
        let user = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        let chain = ChainId::arbitrum();
        let usdc = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xaf88d065e77c8cc2239327c5edb3a432268e5831").unwrap(),
            },
        };
        let weth = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x82af49447d8a07e3bd95bd0d56f35241523fbab1").unwrap(),
            },
        };
        let pool = Address::from_str("0xc6962004f452be9203591991d15f6b388e09e8d0").unwrap();
        let v3 = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool,
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };
        let pool_source = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "slot0()".into(),
            decoder_id: "uniswap_v3_slot0".into(),
        };
        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10000,
                hops: vec![RouteHop {
                    token_in: usdc.clone(),
                    token_out: weth.clone(),
                    venue: v3.clone(),
                    pool_state,
                    effective_fee_bp: 5,
                    estimated_out: U256::from(305_000_000_000_000_000u64),
                }],
                estimated_out: U256::from(305_000_000_000_000_000u64),
            }],
            aggregator: None,
        };
        let swap = AmmAction::Swap(SwapAction {
            venue: v3,
            params: SwapParams {
                token_in: usdc,
                token_out: Some(weth),
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1_000_000_000u64),
                    min_amount_out: U256::from(300_000_000_000_000_000u64),
                },
                recipient: user,
                slippage_bp: 50,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(route, pool_source.clone(), now)
                    .with_ttl(Duration::from_secs(12)),
                expected_amount_out: LiveField::new(
                    U256::from(305_000_000_000_000_000u64),
                    pool_source.clone(),
                    now,
                ),
                price_impact_bp: LiveField::new(12u32, pool_source, now),
                gas_estimate: LiveField::new(
                    U256::from(180_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "gas/arbitrum".into(),
                    },
                    now,
                ),
            },
        });
        let meta = ActionMeta {
            submitted_at: now,
            submitter: user,
            nature: ActionNature::OnchainTx {
                chain,
                nonce: 42,
                gas_limit: U256::from(200_000u64),
                gas_price: LiveField::new(
                    U256::from(100_000_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "ETH/USD".into(),
                    },
                    now,
                ),
                value: U256::ZERO,
            },
        };
        (ActionBody::Amm(swap), meta)
    }

    #[test]
    fn end_to_end_lower_plan_materialize_and_strict_validate() {
        use crate::lowering_v2::{lower_action, TxMeta};

        const FROM: &str = "0x1111111111111111111111111111111111111111";
        const TO: &str = "0x2222222222222222222222222222222222222222";

        let (body, meta) = swap_sample();
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let view = body.view();
        let tx = TxView {
            chain_id: "eip155:42161",
            from: FROM,
            to: TO,
        };
        let manifest = swap_manifest();

        // 1. Plan: the $.action.recipient selector pulls the lowered recipient,
        //    and $.root.chain_id pulls the tx CAIP-2 string.
        let planned = plan_policy_rpc_v2(
            std::slice::from_ref(&manifest),
            &view,
            &lowered.context,
            &tx,
        )
        .unwrap();
        assert_eq!(planned.len(), 1);
        assert_eq!(
            planned[0].call_id,
            "large-swap-usd-warning::total-input-usd"
        );
        let lowered_recipient = lowered.context["recipient"].clone();
        assert_eq!(planned[0].params["recipient"], lowered_recipient);
        assert_eq!(planned[0].params["chain_id"], json!("eip155:42161"));

        // 2. Materialize a simulated oracle result into context.custom.
        let mut context = lowered.context.clone();
        let mut results = BTreeMap::new();
        results.insert(planned[0].call_id.clone(), json!({ "usd": "3500.1200" }));
        materialize_v2(&mut context, &planned, &results).unwrap();
        assert_eq!(
            context["custom"]["totalInputUsd"],
            json!({ "__extn": { "fn": "decimal", "arg": "3500.1200" } })
        );

        // 3. BONUS: compose the per-policy schema (custom field declared),
        //    strict-validate the MATERIALIZED context against the swap uid, then
        //    evaluate a policy that reads context.custom.totalInputUsd.
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(context.clone(), Some((&schema, &uid)))
            .expect("materialized context with custom.totalInputUsd must conform");

        // `custom` is an optional attribute and `totalInputUsd` is a `decimal`
        // extension value, so the guard must `has`-check the path and use the
        // decimal `greaterThan` method (Cedar has no `>` on `decimal`).
        let policy = "@id(\"large-input\")\n@severity(\"warn\")\n\
            forbid(principal, action == Amm::Action::\"Swap\", resource)\n\
            when { context has custom && context.custom has totalInputUsd \
            && context.custom.totalInputUsd.greaterThan(decimal(\"1000.0000\")) };\n";
        let engine =
            crate::policy::PolicyEngine::build_from_per_policy(&[(policy.to_owned(), schema_text)])
                .unwrap();
        let verdict = engine
            .evaluate(
                &lowered.principal,
                &lowered.action_uid,
                &lowered.resource,
                &json!([]),
                &context,
            )
            .unwrap();
        assert!(
            matches!(verdict, crate::policy::Verdict::Warn(_)),
            "3500 USD input must warn, got {verdict:?}"
        );
    }
}
