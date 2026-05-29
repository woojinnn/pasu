//! `#[wasm_bindgen]` v2 (ActionBody-model) policy-RPC exports.
//!
//! Additive counterparts to the v1 exports in [`crate::exports`]
//! ([`plan_policy_rpc_json`](crate::exports::plan_policy_rpc_json) /
//! [`evaluate_policy_rpc_json`](crate::exports::evaluate_policy_rpc_json)),
//! built on the new [`ActionBody`] model instead of the legacy
//! [`ActionEnvelope`](policy_engine::ActionEnvelope). The two phases mirror v1
//! exactly:
//!
//! 1. [`plan_action_rpc_v2_json`] — lower the action, plan the v2 policy-RPC
//!    calls, return `{ planned: [...] }` for the host to dispatch.
//! 2. [`evaluate_action_v2_json`] — lower the action again, replay the host's
//!    raw results into `context.custom`, then evaluate every matching bundle's
//!    Cedar policy against its own per-policy schema and aggregate the verdict.
//!
//! The input JSON reuses the trigger export's `{ manifests, action, tx }`
//! shape, extended with `meta: ActionMeta` (the lowering needs it) and — for
//! the evaluate phase — `bundles: [{ policy, manifest }]` and a raw
//! `results: { call_id: Value }` map.
//!
//! Fail-closed translation of [`PolicyRpcError::SystemFail`] into a synthetic
//! `Verdict::Fail` happens at THIS boundary (via
//! [`system_fail_verdict`](policy_engine::policy_rpc::system_fail_verdict)),
//! mirroring v1's `d9_branch` in `evaluate_policy_rpc_json`.
//!
//! # Boundary invariant — the planned set is derived from the bundles
//!
//! v1 tied PLAN + materialize + the installed engine to ONE manifest set via
//! `manifest_set_hash` / `schema_hash`, so a required RPC call could never be
//! evaluated by a policy that the plan phase did not enrich. v2 has no
//! installed engine to hash against — the policies arrive inline as `bundles`.
//! The equivalent invariant is therefore restored structurally:
//! [`evaluate_action_v2_json`] PLANS and MATERIALIZES from the **bundles' own
//! manifests**, never from a host-supplied side list. Every bundle that is
//! evaluated thus has its required (`optional == false`) calls in the planned
//! set; a missing result for any of them surfaces as
//! [`PolicyRpcError::SystemFail`] → a fail-closed `__system__` verdict. The
//! boundary cannot fail-open by the host passing inconsistent manifest lists,
//! because there is only one list.
//!
//! [`ActionBody`]: simulation_reducer::action::ActionBody

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::wasm_bindgen;

use policy_engine::lowering_v2::{lower_action, LoweredAction, TxMeta};
use policy_engine::policy::{MatchedPolicy, PolicyEngine, Severity, Verdict};
use policy_engine::policy_rpc::{
    plan_policy_rpc_v2, system_fail_verdict, ManifestV2, PlannedCallV2, TxView,
};
use policy_engine::schema::compose_per_policy;
use simulation_reducer::action::{ActionBody, ActionMeta};

use crate::dto::{EngineErrorDto, Envelope, MatchedPolicyDto, VerdictDto};
use crate::exports::check_input_size;

// ── input DTOs ────────────────────────────────────────────────────────────

/// Transaction-level routing fields. Mirrors the trigger export's `TxInput`,
/// reused for both phases. `chain_id` is the CAIP-2 string (e.g. `"eip155:1"`).
#[derive(Debug, Clone, Deserialize)]
struct TxInput {
    chain_id: String,
    from: String,
    to: String,
}

/// Input to [`plan_action_rpc_v2_json`].
///
/// Carries the decoded [`ActionBody`], its [`ActionMeta`], the installed v2
/// manifests, and the tx routing fields. Reuses the trigger export's
/// `{ manifests, action, tx }` shape, extended with `meta` (required by
/// [`lower_action`]).
#[derive(Debug, Deserialize)]
struct PlanActionInput {
    manifests: Vec<ManifestV2>,
    action: ActionBody,
    meta: ActionMeta,
    tx: TxInput,
}

/// One installed bundle: the user's Cedar policy text paired with the manifest
/// that synthesizes its per-policy schema + custom-context.
#[derive(Debug, Deserialize)]
struct BundleInput {
    policy: String,
    manifest: ManifestV2,
}

/// Input to [`evaluate_action_v2_json`].
///
/// Everything [`PlanActionInput`] carries minus `manifests` (the action must be
/// re-lowered to recover the principal/action/resource uids and base context),
/// plus the installed `bundles` and the host's raw `results` keyed by
/// [`PlannedCallV2::call_id`].
///
/// There is deliberately **no** standalone `manifests` field: the planned set
/// that drives materialization (and therefore the `SystemFail` gate) is derived
/// from `bundles[].manifest`, the same manifests that produce the evaluated
/// schemas. See the module-level boundary invariant — a separate `manifests`
/// list would let the host diverge the gate from the evaluated policies and
/// silently fail-open a required RPC call.
#[derive(Debug, Deserialize)]
struct EvaluateActionInput {
    action: ActionBody,
    meta: ActionMeta,
    tx: TxInput,
    bundles: Vec<BundleInput>,
    /// Raw host results keyed by `call_id` (the unwrapped `$.result` payload).
    #[serde(default)]
    results: BTreeMap<String, Value>,
}

// ── output DTOs ──────────────────────────────────────────────────────────

/// Serializable mirror of [`PlannedCallV2`] (the engine type is not `Serialize`).
#[derive(Debug, Clone, Serialize)]
struct PlannedCallDto {
    manifest_id: String,
    call_id: String,
    method: String,
    params: Value,
    /// Output projection rules, rooted at `$.result`, as opaque JSON.
    outputs: Vec<Value>,
    optional: bool,
}

/// Success payload of [`plan_action_rpc_v2_json`].
#[derive(Debug, Clone, Serialize)]
struct PlanActionOutput {
    planned: Vec<PlannedCallDto>,
}

/// Success payload of [`evaluate_action_v2_json`].
#[derive(Debug, Clone, Serialize)]
struct EvaluateActionOutput {
    verdict: VerdictDto,
}

// ── exports ──────────────────────────────────────────────────────────────

/// PLAN phase: lower the action and plan its v2 policy-RPC calls.
///
/// Parses [`PlanActionInput`], lowers via [`lower_action`], builds the
/// [`ActionView`](simulation_reducer::action::ActionView) + [`TxView`], calls
/// [`plan_policy_rpc_v2`], and returns the planned calls inside the standard
/// `{ ok, data }` envelope. The host dispatches each call and returns the raw
/// results keyed by `call_id` to [`evaluate_action_v2_json`].
///
/// The host **should** plan over the same manifest set it later submits as
/// `bundles[].manifest` to [`evaluate_action_v2_json`], so every required call
/// is dispatched. This is advisory only: the plan phase does not gate the
/// verdict. [`evaluate_action_v2_json`] re-plans from the bundles themselves and
/// fail-closes (`__system__`) on any required call whose result is missing, so a
/// plan/evaluate manifest mismatch can never silently fail-open — it can only
/// surface as a fail-closed verdict.
#[wasm_bindgen]
#[must_use]
pub fn plan_action_rpc_v2_json(input_json: String) -> String {
    let result = (|| -> Result<PlanActionOutput, EngineErrorDto> {
        check_input_size(&input_json, "plan_action_rpc_v2_json")?;
        let input: PlanActionInput = serde_json::from_str(&input_json)
            .map_err(|error| invalid_input(&error.to_string()))?;
        let lowered = lower(&input.action, &input.meta, &input.tx)?;
        let planned = plan(&input.manifests, &input.action, &lowered, &input.tx)?;
        Ok(PlanActionOutput {
            planned: planned.iter().map(planned_to_dto).collect(),
        })
    })();

    match result {
        Ok(output) => Envelope::ok(output).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

/// EVALUATE phase: replay the host's raw results into `context.custom`, then
/// evaluate every matching bundle and aggregate the verdict.
///
/// Parses [`EvaluateActionInput`], re-lowers the action (to recover the
/// principal/action/resource uids + base context), plans the calls **from the
/// bundles' own manifests** so the planned set materialized into the context is
/// exactly the set the evaluated policies depend on (see the module-level
/// boundary invariant), writes the host `results` into `context.custom.*`,
/// then — for each bundle whose [`Trigger`](policy_engine::policy_rpc::Trigger)
/// matches the action — composes its per-policy schema, builds a single
/// per-policy engine, and evaluates. The per-bundle verdicts are aggregated by
/// deny-overrides ([`Verdict::aggregate`]).
///
/// A [`PolicyRpcError::SystemFail`] during materialization is translated here
/// into the synthetic `__system__` `Verdict::Fail` (mirroring v1's `d9_branch`);
/// every other error becomes an `__engine::*` `Fail`. The verdict is always
/// returned inside an `ok` envelope, so the host reads `data.verdict.kind`.
#[wasm_bindgen]
#[must_use]
pub fn evaluate_action_v2_json(input_json: String) -> String {
    let verdict = (|| -> Result<Verdict, EngineErrorDto> {
        check_input_size(&input_json, "evaluate_action_v2_json")?;
        let input: EvaluateActionInput = serde_json::from_str(&input_json)
            .map_err(|error| invalid_input(&error.to_string()))?;

        let lowered = lower(&input.action, &input.meta, &input.tx)?;

        // Boundary invariant: PLAN over the bundles' own manifests, never a
        // host-supplied side list. This ties the `SystemFail` gate (driven by
        // the planned set below) to the exact manifests whose schemas/policies
        // are evaluated, so a bundle requiring an un-planned RPC call cannot
        // silently fail-open (v2 analogue of v1's manifest_set_hash tie).
        let manifests: Vec<ManifestV2> =
            input.bundles.iter().map(|b| b.manifest.clone()).collect();
        let planned = plan(&manifests, &input.action, &lowered, &input.tx)?;

        // Replay the host's raw results into context.custom.* . A required
        // call that is missing / fails projection surfaces as `SystemFail`,
        // which we translate to a fail-closed verdict at this boundary
        // (mirroring v1's `evaluate_policy_rpc_json` D9 branch).
        let mut context = lowered.context.clone();
        if let Err(error) =
            policy_engine::policy_rpc::materialize_v2(&mut context, &planned, &input.results)
        {
            if let Some(verdict) = system_fail_verdict(&error) {
                return Ok(verdict);
            }
            return Err(EngineErrorDto::new("projection_failed", error.to_string()));
        }

        evaluate_matching_bundles(&input.bundles, &input.action, &input.tx, &lowered, &context)
    })();

    let dto = match verdict {
        Ok(verdict) => verdict_to_dto(&verdict),
        Err(error) => engine_error_verdict(error),
    };
    Envelope::ok(EvaluateActionOutput { verdict: dto }).to_json()
}

// ── shared helpers ───────────────────────────────────────────────────────

/// Lower an [`ActionBody`] + [`ActionMeta`] + tx into a [`LoweredAction`].
fn lower(
    action: &ActionBody,
    meta: &ActionMeta,
    tx: &TxInput,
) -> Result<LoweredAction, EngineErrorDto> {
    let tx_meta = TxMeta {
        from: &tx.from,
        to: &tx.to,
    };
    lower_action(action, meta, &tx_meta)
        .map_err(|error| EngineErrorDto::new("unsupported_action", error.to_string()))
}

/// Plan the v2 policy-RPC calls for one lowered action.
fn plan(
    manifests: &[ManifestV2],
    action: &ActionBody,
    lowered: &LoweredAction,
    tx: &TxInput,
) -> Result<Vec<PlannedCallV2>, EngineErrorDto> {
    let view = action.view();
    let tx_view = tx_view(tx);
    plan_policy_rpc_v2(manifests, &view, &lowered.context, &tx_view)
        .map_err(|error| EngineErrorDto::new("plan_failed", error.to_string()))
}

/// Evaluate every bundle whose trigger matches the action and aggregate the
/// per-bundle verdicts (deny-overrides via [`Verdict::aggregate`]).
///
/// A bundle whose [`Trigger`](policy_engine::policy_rpc::Trigger) does not match
/// the action is skipped (it neither contributes a verdict nor an error). With
/// no matching bundles the aggregate of an empty list is `Pass` — the
/// no-manifest baseline.
fn evaluate_matching_bundles(
    bundles: &[BundleInput],
    action: &ActionBody,
    tx: &TxInput,
    lowered: &LoweredAction,
    context: &Value,
) -> Result<Verdict, EngineErrorDto> {
    let view = action.view();
    let tx_view = tx_view(tx);

    let mut verdicts: Vec<Verdict> = Vec::new();
    for bundle in bundles {
        bundle
            .manifest
            .validate()
            .map_err(|error| EngineErrorDto::new("invalid_manifest", error.to_string()))?;
        if !policy_engine::policy_rpc::evaluate_trigger(&bundle.manifest.trigger, &view, &tx_view) {
            continue;
        }

        let schema = compose_per_policy(&bundle.manifest)
            .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))?;
        let engine = PolicyEngine::build_from_per_policy(&[(bundle.policy.clone(), schema)])
            .map_err(|error| EngineErrorDto::new("install_failed", error.to_string()))?;
        let verdict = engine
            .evaluate(
                &lowered.principal,
                &lowered.action_uid,
                &lowered.resource,
                &Value::Array(Vec::new()),
                context,
            )
            .map_err(|error| EngineErrorDto::new("policy", error.to_string()))?;
        verdicts.push(verdict);
    }

    Ok(Verdict::aggregate(verdicts))
}

/// Build a borrowed [`TxView`] from the parsed `tx` input.
fn tx_view(tx: &TxInput) -> TxView<'_> {
    TxView {
        chain_id: &tx.chain_id,
        from: &tx.from,
        to: &tx.to,
    }
}

fn planned_to_dto(call: &PlannedCallV2) -> PlannedCallDto {
    PlannedCallDto {
        manifest_id: call.manifest_id.clone(),
        call_id: call.call_id.clone(),
        method: call.method.clone(),
        params: call.params.clone(),
        outputs: call
            .outputs
            .iter()
            .map(|output| serde_json::to_value(output).unwrap_or(Value::Null))
            .collect(),
        optional: call.optional,
    }
}

fn invalid_input(message: &str) -> EngineErrorDto {
    EngineErrorDto::new("invalid_input_json", format!("invalid input json: {message}"))
}

// ── verdict → DTO mapping (local mirror of `exports.rs`) ──────────────────

fn verdict_to_dto(verdict: &Verdict) -> VerdictDto {
    match verdict {
        Verdict::Pass => VerdictDto::Pass,
        Verdict::Warn(matched) => VerdictDto::Warn {
            matched: matched.iter().map(matched_to_dto).collect(),
        },
        Verdict::Fail(matched) => VerdictDto::Fail {
            matched: matched.iter().map(matched_to_dto).collect(),
        },
    }
}

fn matched_to_dto(matched: &MatchedPolicy) -> MatchedPolicyDto {
    MatchedPolicyDto {
        policy_id: matched.policy_id.clone(),
        reason: matched.reason.clone(),
        severity: match matched.severity {
            Severity::Deny => "deny".to_owned(),
            Severity::Warn => "warn".to_owned(),
        },
        origin: match matched.origin {
            policy_engine::PolicyRequestOrigin::Action => "action".to_owned(),
            policy_engine::PolicyRequestOrigin::Tx => "tx".to_owned(),
        },
    }
}

/// Translate an engine-level error into a fail-closed `Verdict::Fail` carrying a
/// synthetic `__engine::<kind>` matched policy. Mirrors `exports::engine_error_verdict`.
fn engine_error_verdict(error: EngineErrorDto) -> VerdictDto {
    let policy_id = format!("__engine::{}", error.kind);
    let reason = if error.message.is_empty() {
        policy_id.clone()
    } else {
        error.message
    };
    VerdictDto::Fail {
        matched: vec![MatchedPolicyDto {
            policy_id,
            reason: Some(reason),
            severity: "deny".to_owned(),
            // Match v1's `exports::engine_error_verdict` contract: the synthetic
            // `__engine::*` Fail carries origin "engine_error", not "action".
            origin: "engine_error".to_owned(),
        }],
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::str::FromStr;

    use simulation_reducer::action::amm::{
        AmmAction, AmmVenue, PoolState, RouteHop, RoutePath, SwapAction, SwapDirection,
        SwapLiveInputs, SwapParams, SwapRoute,
    };
    use simulation_reducer::action::{ActionMeta, ActionNature};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Duration, Time, U128, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::LiveField;

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    /// A faithful UniswapV3 swap `ActionBody` + `ActionMeta` (mirrors the
    /// `materialize_v2` reference fixture).
    fn swap_sample() -> (ActionBody, ActionMeta) {
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
                token_out: weth,
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

    /// A swap manifest: trigger matches `swap`, one policy_rpc call writing
    /// `context.custom.totalInputUsd` (decimal), declared in `custom_context`.
    fn swap_manifest() -> Value {
        json!({
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
        })
    }

    /// A Cedar policy that warns when `context.custom.totalInputUsd` exceeds
    /// 1000. `custom` is optional and `totalInputUsd` is a `decimal` extension
    /// value, so the guard must `has`-check the path and use `greaterThan`.
    fn warn_policy() -> &'static str {
        "@id(\"large-input\")\n@severity(\"warn\")\n\
         @reason(\"large USD input\")\n\
         forbid(principal, action == Amm::Action::\"Swap\", resource)\n\
         when { context has custom && context.custom has totalInputUsd \
         && context.custom.totalInputUsd.greaterThan(decimal(\"1000.0000\")) };\n"
    }

    fn tx() -> Value {
        json!({ "chain_id": "eip155:42161", "from": FROM, "to": TO })
    }

    #[test]
    fn plan_action_rpc_v2_returns_oracle_call() {
        let (body, meta) = swap_sample();
        let input = json!({
            "manifests": [swap_manifest()],
            "action": body,
            "meta": meta,
            "tx": tx(),
        });
        let out = plan_action_rpc_v2_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        let planned = parsed["data"]["planned"].as_array().expect("planned array");
        assert_eq!(planned.len(), 1, "{parsed}");
        assert_eq!(planned[0]["call_id"], "large-swap-usd-warning::total-input-usd");
        assert_eq!(planned[0]["method"], "oracle.usd_value");
        assert_eq!(planned[0]["params"]["chain_id"], "eip155:42161");
    }

    /// End-to-end: plan → simulate an oracle result → evaluate → Warn.
    #[test]
    fn evaluate_action_v2_warns_on_large_input() {
        let (body, meta) = swap_sample();

        // 1. PLAN — recover the call_id the host must key its result under.
        let plan_out = plan_action_rpc_v2_json(
            json!({
                "manifests": [swap_manifest()],
                "action": body,
                "meta": meta,
                "tx": tx(),
            })
            .to_string(),
        );
        let plan_parsed: Value = serde_json::from_str(&plan_out).unwrap();
        let call_id = plan_parsed["data"]["planned"][0]["call_id"]
            .as_str()
            .expect("call_id")
            .to_owned();
        assert_eq!(call_id, "large-swap-usd-warning::total-input-usd");

        // 2. EVALUATE — the host returns a $3500 oracle valuation, which the
        //    warn policy (threshold 1000) trips. The evaluate phase plans from
        //    the bundle's own manifest, so the planned call_id matches the one
        //    the plan phase produced and the host keyed its result under.
        let eval_out = evaluate_action_v2_json(
            json!({
                "action": body,
                "meta": meta,
                "tx": tx(),
                "bundles": [{ "policy": warn_policy(), "manifest": swap_manifest() }],
                "results": { call_id: { "usd": "3500.1200" } }
            })
            .to_string(),
        );
        let eval_parsed: Value = serde_json::from_str(&eval_out).unwrap();
        assert_eq!(eval_parsed["ok"], true, "{eval_parsed}");
        assert_eq!(eval_parsed["data"]["verdict"]["kind"], "warn", "{eval_parsed}");
        assert_eq!(
            eval_parsed["data"]["verdict"]["matched"][0]["policy_id"], "large-input",
            "{eval_parsed}"
        );
    }

    /// No bundles installed → the aggregate of zero verdicts is `Pass`.
    #[test]
    fn evaluate_action_v2_no_bundle_baseline_passes() {
        let (body, meta) = swap_sample();
        let eval_out = evaluate_action_v2_json(
            json!({
                "action": body,
                "meta": meta,
                "tx": tx(),
                "bundles": [],
                "results": {}
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&eval_out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["verdict"]["kind"], "pass", "{parsed}");
    }

    /// A required call with no host result fails closed: `materialize_v2`
    /// returns `SystemFail`, which this boundary maps to a `__system__`
    /// `Verdict::Fail` (mirrors v1's D9 branch).
    #[test]
    fn evaluate_action_v2_missing_required_result_system_fails() {
        let (body, meta) = swap_sample();
        let eval_out = evaluate_action_v2_json(
            json!({
                "action": body,
                "meta": meta,
                "tx": tx(),
                "bundles": [{ "policy": warn_policy(), "manifest": swap_manifest() }],
                // No result for the required call → SystemFail.
                "results": {}
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&eval_out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["verdict"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["verdict"]["matched"][0]["policy_id"], "__system__",
            "{parsed}"
        );
    }

    /// Regression for the divergent-manifest fail-open (Task #7 review,
    /// high). Before the fix, `evaluate_action_v2_json` drove the SystemFail
    /// gate off a standalone `manifests` list while evaluating a SEPARATE
    /// `bundles[].manifest`. A bundle whose required RPC manifest was *absent*
    /// from `manifests` was never planned, never materialized, never
    /// SystemFailed — and the has-guarded forbid reading the absent custom
    /// field short-circuited to Pass (fail-open).
    ///
    /// The fix derives the planned set from `bundles[].manifest`, so there is
    /// no second list to diverge: a bundle requiring an RPC call whose result
    /// the host never returns now ALWAYS SystemFails to a `__system__` Fail.
    /// Here we reproduce the historical attack shape — a (now-ignored)
    /// `manifests` side list that does NOT contain the bundle's manifest, with
    /// empty `results` — and assert it fails closed.
    #[test]
    fn evaluate_action_v2_divergent_manifest_fails_closed_not_open() {
        let (body, meta) = swap_sample();
        let eval_out = evaluate_action_v2_json(
            json!({
                // Historical fail-open vector: a side list that does NOT carry
                // the bundle's manifest. It is now ignored entirely — the
                // planned set comes from `bundles[].manifest`.
                "manifests": [],
                "action": body,
                "meta": meta,
                "tx": tx(),
                "bundles": [{ "policy": warn_policy(), "manifest": swap_manifest() }],
                // Host returned nothing for the bundle's required call.
                "results": {}
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&eval_out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["verdict"]["kind"], "fail",
            "divergent manifest must fail closed, not Pass: {parsed}"
        );
        assert_eq!(
            parsed["data"]["verdict"]["matched"][0]["policy_id"], "__system__",
            "{parsed}"
        );
    }

    #[test]
    fn invalid_input_returns_error_envelope() {
        let out = plan_action_rpc_v2_json("not json".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json", "{parsed}");
    }
}
