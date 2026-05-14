//! Policy-rpc result projection into Cedar contexts.

use super::planning::{action_fields_json, policy_rpc_call_id};
use super::{
    resolve_selector, PolicyManifest, PolicyRpcError, PolicyRpcResponse, PolicyRpcResult,
    ProjectionType,
};
use crate::action::ActionEnvelope;
use crate::core::UsdValuation;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

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
    for output in &requirement.outputs {
        if output.kind != "context" {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "unsupported projection kind `{}`",
                output.kind
            )));
        }
        validate_declared_context_projection(manifest, action_kind, output)?;

        let Some(result) = result else {
            if output.required {
                return Err(PolicyRpcError::RpcResult(format!(
                    "required rpc result `{call_id}` is missing"
                )));
            }
            continue;
        };

        if !result.ok {
            if output.required {
                let message = result
                    .error
                    .as_ref()
                    .map_or("required rpc result failed", |error| error.message.as_str());
                return Err(PolicyRpcError::RpcResult(format!(
                    "required rpc result `{call_id}` failed: {message}"
                )));
            }
            continue;
        }

        let Some(payload) = &result.result else {
            if output.required {
                return Err(PolicyRpcError::RpcResult(format!(
                    "required rpc result `{call_id}` has no payload"
                )));
            }
            continue;
        };

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
            Err(error) if output.required => return Err(error),
            Err(_) => continue,
        };
        let context = request.context.as_object_mut().ok_or_else(|| {
            PolicyRpcError::RpcResult("policy request context is not an object".to_owned())
        })?;
        if context.contains_key(&output.field) {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "context projection `{action_kind}.{}` would overwrite an existing context field",
                output.field
            )));
        }
        context.insert(output.field.clone(), materialized);
    }

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
