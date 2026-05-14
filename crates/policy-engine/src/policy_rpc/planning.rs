//! Policy-rpc call planning.

use super::{
    resolve_selector, validate_manifests, PolicyManifest, PolicyRpcCall, PolicyRpcError, RootInput,
};
use crate::action::ActionEnvelope;
use crate::{ActionAddress, DecimalString};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Build deterministic manifest-set hash.
#[must_use]
pub fn manifest_set_hash(manifests: &[PolicyManifest]) -> String {
    let mut canonical = manifests.to_vec();
    canonical.sort_by(|left, right| left.id.cmp(&right.id));
    let json = match serde_json::to_vec(&canonical) {
        Ok(json) => json,
        Err(error) => error.to_string().into_bytes(),
    };
    let digest = Sha256::digest(json);
    format!("sha256:{digest:x}")
}

/// Plan policy-rpc calls for routed action envelopes.
///
/// V0 supports only per-action requirements and selector-based params.
///
/// # Errors
///
/// Returns an error when action serialization or selector resolution fails.
pub fn plan_calls(
    root: &RootInput,
    envelopes: &[ActionEnvelope],
    manifests: &[PolicyManifest],
    params_root: &Value,
) -> Result<Vec<PolicyRpcCall>, PolicyRpcError> {
    validate_manifests(manifests)?;
    let root_json = root.to_selector_json();
    let mut calls = Vec::new();

    for (envelope_index, envelope) in envelopes.iter().enumerate() {
        let action_kind = envelope.action.kind();
        let action_json = action_fields_json(envelope)?;
        let context_json = action_context_json(root, envelope)?;
        for manifest in manifests {
            for requirement in &manifest.requires {
                if requirement.when.action != action_kind {
                    continue;
                }
                let mut params = Map::new();
                for (key, template) in &requirement.params {
                    let value = match template {
                        Value::String(selector) if selector.starts_with("$.") => resolve_selector(
                            selector,
                            &root_json,
                            &action_json,
                            &context_json,
                            &Value::Object(Map::new()),
                            params_root,
                        )?,
                        literal => literal.clone(),
                    };
                    params.insert(key.clone(), value);
                }
                calls.push(PolicyRpcCall {
                    id: policy_rpc_call_id(&manifest.id, envelope_index, &requirement.id),
                    method: requirement.method.clone(),
                    params: Value::Object(params),
                });
            }
        }
    }

    Ok(calls)
}

pub(crate) fn policy_rpc_call_id(
    manifest_id: &str,
    envelope_index: usize,
    requirement_id: &str,
) -> String {
    format!("{manifest_id}::{envelope_index}::{requirement_id}")
}

pub(crate) fn action_fields_json(envelope: &ActionEnvelope) -> Result<Value, PolicyRpcError> {
    serde_json::to_value(envelope)
        .map_err(|error| PolicyRpcError::InvalidManifest(error.to_string()))?
        .get("fields")
        .cloned()
        .ok_or_else(|| PolicyRpcError::InvalidManifest("action envelope has no fields".to_owned()))
}

fn action_context_json(
    root: &RootInput,
    envelope: &ActionEnvelope,
) -> Result<Value, PolicyRpcError> {
    let from = root
        .from
        .parse::<ActionAddress>()
        .map_err(|error| PolicyRpcError::InvalidManifest(format!("invalid root.from: {error}")))?;
    let to = root
        .to
        .parse::<ActionAddress>()
        .map_err(|error| PolicyRpcError::InvalidManifest(format!("invalid root.to: {error}")))?;
    let value_wei = root.value_wei.parse::<DecimalString>().map_err(|error| {
        PolicyRpcError::InvalidManifest(format!("invalid root.value_wei: {error}"))
    })?;
    let Some(request) = crate::policy_request_from_envelope(
        envelope,
        &from,
        &to,
        &value_wei,
        root.chain_id,
        root.block_timestamp.unwrap_or_default(),
    ) else {
        return Ok(Value::Object(Map::new()));
    };
    Ok(request.context)
}
