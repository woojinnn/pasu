//! Policy-rpc result projection into Cedar contexts.

use super::planning::{action_fields_json, policy_rpc_call_id};
use super::{
    resolve_selector, PolicyManifest, PolicyRpcError, PolicyRpcResponse, PolicyRpcResult,
    ProjectionType,
};
use crate::action::ActionEnvelope;
use crate::core::UsdValuation;
use crate::policy::{MatchedPolicy, PolicyRequest, PolicyRequestOrigin, Severity, Verdict};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// D9 synthetic policy id.
///
/// Reported when a non-optional manifest requirement fails to materialize.
/// Callers that need a verdict-shaped result (rather than the raw
/// `PolicyRpcError`) should funnel `SystemFail` through
/// [`system_fail_verdict`].
pub const SYSTEM_POLICY_ID: &str = "__system__";

/// Translate `PolicyRpcError::SystemFail` into the D9 synthetic verdict.
///
/// Returns `Verdict::Fail` containing a single `MatchedPolicy` whose
/// `policy_id` is [`SYSTEM_POLICY_ID`] and whose `reason` is
/// `"rpc-unavailable: <call_id>"`. Returns `None` for any other variant so
/// the caller can propagate non-D9 failures as engine errors.
#[must_use]
pub fn system_fail_verdict(error: &PolicyRpcError) -> Option<Verdict> {
    if let PolicyRpcError::SystemFail { call_id, .. } = error {
        Some(Verdict::Fail(vec![MatchedPolicy {
            policy_id: SYSTEM_POLICY_ID.to_owned(),
            reason: Some(format!("rpc-unavailable: {call_id}")),
            severity: Severity::Deny,
            origin: PolicyRequestOrigin::Action,
        }]))
    } else {
        None
    }
}

/// Apply policy-rpc results to lowered policy request contexts.
///
/// The current v0 assumes one lowered request for each policy-evaluable action
/// envelope and applies context projections in matching order.
///
/// # Errors
///
/// Returns an error when a required result is missing or failed, a selector
/// cannot be resolved, or a projected value does not match its declared type.
pub fn apply_rpc_results(
    requests: &mut [PolicyRequest],
    envelopes: &[ActionEnvelope],
    manifests: &[PolicyManifest],
    response: &PolicyRpcResponse,
) -> Result<(), PolicyRpcError> {
    let indexed_envelopes = envelopes.iter().enumerate().collect::<Vec<_>>();
    apply_rpc_results_with_indices(requests, &indexed_envelopes, manifests, response)
}

/// Apply policy-rpc results when each lowered request is paired with its
/// original routed action index from the plan.
///
/// # Errors
///
/// Returns an error when response ids are duplicated, unexpected, or missing,
/// or when a required projection cannot be materialized.
pub fn apply_rpc_results_with_indices(
    requests: &mut [PolicyRequest],
    indexed_envelopes: &[(usize, &ActionEnvelope)],
    manifests: &[PolicyManifest],
    response: &PolicyRpcResponse,
) -> Result<(), PolicyRpcError> {
    if requests.len() != indexed_envelopes.len() {
        return Err(PolicyRpcError::RpcResult(format!(
            "request/envelope length mismatch: {} requests, {} envelopes",
            requests.len(),
            indexed_envelopes.len()
        )));
    }

    let expected_ids = expected_call_ids(indexed_envelopes, manifests);
    let mut results = HashMap::new();
    for result in &response.results {
        if !expected_ids.contains(&result.id) {
            return Err(PolicyRpcError::RpcResult(format!(
                "unexpected result id `{}`",
                result.id
            )));
        }
        if results.insert(result.id.as_str(), result).is_some() {
            return Err(PolicyRpcError::RpcResult(format!(
                "duplicate result id `{}`",
                result.id
            )));
        }
    }
    for expected_id in &expected_ids {
        if !results.contains_key(expected_id.as_str()) {
            return Err(PolicyRpcError::RpcResult(format!(
                "missing result id `{expected_id}`"
            )));
        }
    }

    for (request, (envelope_index, envelope)) in requests.iter_mut().zip(indexed_envelopes.iter()) {
        let action_kind = envelope.action.kind();
        let action_json = action_fields_json(envelope)?;
        for manifest in manifests {
            for requirement in &manifest.requires {
                if requirement.when.action != action_kind {
                    continue;
                }
                let call_id = policy_rpc_call_id(&manifest.id, *envelope_index, &requirement.id);
                let result = results.get(call_id.as_str()).copied();
                apply_requirement_result(
                    request,
                    &action_json,
                    manifest,
                    requirement,
                    action_kind,
                    &call_id,
                    result,
                )?;
            }
        }
    }

    Ok(())
}

fn expected_call_ids(
    indexed_envelopes: &[(usize, &ActionEnvelope)],
    manifests: &[PolicyManifest],
) -> HashSet<String> {
    let mut expected_ids = HashSet::new();
    for (envelope_index, envelope) in indexed_envelopes {
        let action_kind = envelope.action.kind();
        for manifest in manifests {
            for requirement in &manifest.requires {
                if requirement.when.action == action_kind {
                    expected_ids.insert(policy_rpc_call_id(
                        &manifest.id,
                        *envelope_index,
                        &requirement.id,
                    ));
                }
            }
        }
    }
    expected_ids
}

fn apply_requirement_result(
    request: &mut PolicyRequest,
    action_json: &Value,
    manifest: &PolicyManifest,
    requirement: &super::Requirement,
    action_kind: &str,
    call_id: &str,
    result: Option<&PolicyRpcResult>,
) -> Result<(), PolicyRpcError> {
    // D9: per-requirement failure model. Once we've decided the requirement is
    // unavailable we either return SystemFail (optional=false) or skip every
    // output the requirement would have produced (optional=true).
    let payload = match result {
        None => return d9_branch(requirement, call_id, "missing rpc result".to_owned()),
        Some(result) if !result.ok => {
            let reason = result
                .error
                .as_ref()
                .map_or_else(|| "rpc result not ok".to_owned(), |e| e.message.clone());
            return d9_branch(requirement, call_id, reason);
        }
        Some(result) => match result.result.as_ref() {
            None => {
                return d9_branch(requirement, call_id, "rpc result has no payload".to_owned())
            }
            Some(payload) => payload,
        },
    };

    for output in &requirement.outputs {
        if output.kind != "context" {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "unsupported projection kind `{}`",
                output.kind
            )));
        }
        validate_declared_context_projection(manifest, action_kind, output)?;

        let materialized = match resolve_selector(
            &output.from,
            &Value::Object(Map::new()),
            action_json,
            &request.context,
            payload,
            &Value::Object(Map::new()),
        )
        .and_then(|selected| materialize_value(&selected, &output.type_name))
        {
            Ok(materialized) => materialized,
            Err(error) => {
                // D9: a per-output projection failure (selector miss / type
                // coercion) is itself a requirement-level failure. Branch on
                // `requirement.optional` rather than the legacy
                // `output.required` discriminator.
                if requirement.optional {
                    continue;
                }
                return Err(PolicyRpcError::SystemFail {
                    call_id: call_id.to_owned(),
                    reason: error.to_string(),
                });
            }
        };
        insert_custom_field(request, action_kind, &output.field, materialized)?;
    }

    Ok(())
}

fn d9_branch(
    requirement: &super::Requirement,
    call_id: &str,
    reason: String,
) -> Result<(), PolicyRpcError> {
    if requirement.optional {
        // optional=true → omit projected fields, evaluation continues.
        Ok(())
    } else {
        Err(PolicyRpcError::SystemFail {
            call_id: call_id.to_owned(),
            reason,
        })
    }
}

/// D3: write the projected field under `context.custom.<field>`, allocating
/// the nested `custom` record on first use.
fn insert_custom_field(
    request: &mut PolicyRequest,
    action_kind: &str,
    field: &str,
    value: Value,
) -> Result<(), PolicyRpcError> {
    let context = request.context.as_object_mut().ok_or_else(|| {
        PolicyRpcError::RpcResult("policy request context is not an object".to_owned())
    })?;
    let custom = context
        .entry("custom")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| PolicyRpcError::RpcResult("context.custom is not an object".to_owned()))?;
    if custom.contains_key(field) {
        return Err(PolicyRpcError::InvalidManifest(format!(
            "context projection `{action_kind}.custom.{field}` would overwrite an existing context field"
        )));
    }
    custom.insert(field.to_owned(), value);
    Ok(())
}

fn validate_declared_context_projection(
    manifest: &PolicyManifest,
    action_kind: &str,
    output: &super::ContextProjection,
) -> Result<(), PolicyRpcError> {
    let declared_type = manifest
        .context_extensions
        .get(action_kind)
        .and_then(|fields| fields.get(&output.field))
        .ok_or_else(|| {
            PolicyRpcError::InvalidManifest(format!(
                "undeclared context projection `{action_kind}.{}` in manifest `{}`",
                output.field, manifest.id
            ))
        })?;
    if canonical_manifest_type(declared_type)? != output.type_name.cedar_type() {
        return Err(PolicyRpcError::InvalidManifest(format!(
            "context projection `{action_kind}.{}` has type {}, but context_extensions declares {declared_type}",
            output.field,
            output.type_name.cedar_type()
        )));
    }
    Ok(())
}

fn canonical_manifest_type(type_name: &str) -> Result<&'static str, PolicyRpcError> {
    match type_name {
        "String" => Ok("String"),
        "Long" => Ok("Long"),
        "Bool" => Ok("Bool"),
        "decimal" | "Decimal" => Ok("decimal"),
        "UsdValuation" => Ok("UsdValuation"),
        "WindowStats" => Ok("WindowStats"),
        "Set<String>" => Ok("Set<String>"),
        other => Err(PolicyRpcError::InvalidManifest(format!(
            "unsupported context field type `{other}`"
        ))),
    }
}

fn materialize_value(value: &Value, type_name: &ProjectionType) -> Result<Value, PolicyRpcError> {
    match type_name {
        ProjectionType::String => value
            .as_str()
            .map(Value::from)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected String".to_owned())),
        ProjectionType::Long => value
            .as_i64()
            .map(Value::from)
            .or_else(|| {
                value
                    .as_u64()
                    .and_then(|value| i64::try_from(value).ok())
                    .map(Value::from)
            })
            .ok_or_else(|| PolicyRpcError::RpcResult("expected Long".to_owned())),
        ProjectionType::Bool => value
            .as_bool()
            .map(Value::from)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected Bool".to_owned())),
        ProjectionType::Decimal => value
            .as_str()
            .map(crate::cedar_json::decimal_json)
            .ok_or_else(|| PolicyRpcError::RpcResult("expected Decimal string".to_owned())),
        ProjectionType::UsdValuation => usd_valuation_from_json(value),
        ProjectionType::WindowStats => window_stats_from_json(value),
        ProjectionType::SetString => {
            let array = value.as_array().ok_or_else(|| {
                PolicyRpcError::RpcResult("expected Set<String> array".to_owned())
            })?;
            let mut out = Vec::with_capacity(array.len());
            for entry in array {
                let Some(entry) = entry.as_str() else {
                    return Err(PolicyRpcError::RpcResult(
                        "expected Set<String> entry string".to_owned(),
                    ));
                };
                out.push(Value::from(entry));
            }
            Ok(Value::Array(out))
        }
    }
}

fn usd_valuation_from_json(value: &Value) -> Result<Value, PolicyRpcError> {
    let object = value
        .as_object()
        .ok_or_else(|| PolicyRpcError::RpcResult("expected UsdValuation object".to_owned()))?;
    let value = object
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| PolicyRpcError::RpcResult("UsdValuation.value must be string".to_owned()))?;
    let as_of_ts = object
        .get("asOfTs")
        .and_then(Value::as_u64)
        .ok_or_else(|| PolicyRpcError::RpcResult("UsdValuation.asOfTs must be u64".to_owned()))?;
    let stale_sec = object
        .get("staleSec")
        .and_then(Value::as_u64)
        .ok_or_else(|| PolicyRpcError::RpcResult("UsdValuation.staleSec must be u64".to_owned()))?;
    let sources = object
        .get("sources")
        .and_then(Value::as_array)
        .ok_or_else(|| PolicyRpcError::RpcResult("UsdValuation.sources must be array".to_owned()))?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| PolicyRpcError::RpcResult("source must be string".to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(crate::cedar_json::usd_valuation_json(&UsdValuation {
        value: value.to_owned(),
        as_of_ts,
        stale_sec,
        sources,
    }))
}

fn window_stats_from_json(value: &Value) -> Result<Value, PolicyRpcError> {
    let object = value
        .as_object()
        .ok_or_else(|| PolicyRpcError::RpcResult("expected WindowStats object".to_owned()))?;
    let mut out = Map::new();

    if let Some(volume) = object.get("swapVolumeUsd24h") {
        let volume = volume.as_str().ok_or_else(|| {
            PolicyRpcError::RpcResult(
                "expected WindowStats.swapVolumeUsd24h decimal string".to_owned(),
            )
        })?;
        out.insert(
            "swapVolumeUsd24h".to_owned(),
            crate::cedar_json::decimal_json(volume),
        );
    }

    if let Some(count) = object.get("swapCount24h") {
        let count = long_from_json(count).ok_or_else(|| {
            PolicyRpcError::RpcResult("expected WindowStats.swapCount24h Long".to_owned())
        })?;
        out.insert("swapCount24h".to_owned(), Value::from(count));
    }

    for field in object.keys() {
        if field != "swapVolumeUsd24h" && field != "swapCount24h" {
            return Err(PolicyRpcError::RpcResult(format!(
                "unexpected WindowStats field `{field}`"
            )));
        }
    }

    Ok(Value::Object(out))
}

fn long_from_json(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
}
