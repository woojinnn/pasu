//! v2 policy-rpc planning over the new [`ActionBody`] model.
//!
//! Additive counterpart to v1 [`super::planning::plan_calls`]. Where v1 keys
//! requirements by `requirement.when.action == envelope.action.kind()` and binds
//! `$.action` to the legacy envelope `fields`, v2 keys whole manifests by the
//! declarative [`Trigger`] (evaluated against an [`ActionView`] + [`TxView`]) and
//! binds `$.action` to the *lowered Cedar action-context JSON* produced by
//! [`crate::lowering_v2::lower_action`].
//!
//! [`ActionBody`]: policy_transition::action::ActionBody

use policy_transition::action::ActionView;
use serde_json::{Map, Value};

use super::manifest_v2::ManifestV2;
use super::trigger::{evaluate as evaluate_trigger, TxView};
use super::{resolve_selector, ContextProjection, PolicyRpcError};

/// One resolved v2 policy-rpc call, ready to dispatch.
///
/// The `params` here are fully **resolved** (every `$.…` selector replaced with
/// its concrete JSON value), unlike the manifest's [`super::PolicyRpcCallSpec`]
/// template. `call_id` is the stable key under which the host returns this
/// call's raw result and under which [`super::materialize_v2`] looks it back up;
/// it is namespaced `"<manifest_id>::<spec_id>"` so two manifests declaring the
/// same spec id never collide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedCallV2 {
    /// Originating manifest id.
    pub manifest_id: String,
    /// Stable call id, `"<manifest_id>::<spec_id>"`.
    pub call_id: String,
    /// Remote method name (opaque).
    pub method: String,
    /// Resolved parameters (selectors already substituted).
    pub params: Value,
    /// Output projection rules, rooted at `$.result`.
    pub outputs: Vec<ContextProjection>,
    /// When true, a missing param selector skips this call instead of failing,
    /// and a missing/failed result is absorbed in [`super::materialize_v2`].
    pub optional: bool,
}

/// The stable call id for a v2 planned call: `"<manifest_id>::<spec_id>"`.
///
/// Used as the map key in three places — the planned call, the host's
/// `results` map, and the materialize lookup — which must all agree.
pub(crate) fn policy_rpc_call_id_v2(manifest_id: &str, spec_id: &str) -> String {
    format!("{manifest_id}::{spec_id}")
}

/// Plan v2 policy-rpc calls for one lowered action.
///
/// For every manifest whose [`Trigger`] matches `action_view` + `tx`, resolve
/// each [`super::PolicyRpcCallSpec`]'s param selectors and emit one
/// [`PlannedCallV2`]. Selector roots:
/// - `$.root` → transaction-level fields (`chain_id` / `from` / `to`, from `tx`);
/// - `$.action` → `lowered_context` (the Cedar action-context JSON);
/// - `$.context` / `$.result` / `$.params` → empty during planning (mirrors v1).
///
/// [`Trigger`]: super::manifest_v2::Trigger
///
/// # Errors
///
/// Returns [`PolicyRpcError::InvalidManifest`] if any manifest fails
/// [`ManifestV2::validate`], or [`PolicyRpcError::Selector`] if a *required*
/// (`optional == false`) param selector cannot be resolved. A failed selector on
/// an `optional` call skips that call instead.
pub fn plan_policy_rpc_v2(
    manifests: &[ManifestV2],
    action_view: &ActionView<'_>,
    lowered_context: &Value,
    tx: &TxView<'_>,
) -> Result<Vec<PlannedCallV2>, PolicyRpcError> {
    let root_json = root_selector_json(tx);
    let empty = Value::Object(Map::new());
    let mut calls = Vec::new();

    for manifest in manifests {
        manifest.validate()?;
        if !evaluate_trigger(&manifest.trigger, action_view, tx) {
            continue;
        }
        for spec in &manifest.policy_rpc {
            let mut params = Map::new();
            let mut skip = false;
            for (key, template) in &spec.params {
                let resolved = match template {
                    Value::String(selector) if selector.starts_with("$.") => resolve_selector(
                        selector,
                        &root_json,
                        lowered_context,
                        &empty,
                        &empty,
                        &empty,
                    ),
                    literal => Ok(literal.clone()),
                };
                match resolved {
                    Ok(value) => {
                        params.insert(key.clone(), value);
                    }
                    Err(error) => {
                        if spec.optional {
                            skip = true;
                            break;
                        }
                        return Err(error);
                    }
                }
            }
            if skip {
                continue;
            }
            calls.push(PlannedCallV2 {
                manifest_id: manifest.id.clone(),
                call_id: policy_rpc_call_id_v2(&manifest.id, &spec.id),
                method: spec.method.clone(),
                params: Value::Object(params),
                outputs: spec.outputs.clone(),
                optional: spec.optional,
            });
        }
    }

    Ok(calls)
}

/// Build the `$.root` selector object from transaction metadata. v2's root is
/// derived purely from [`TxView`] — `chain_id` is the CAIP-2 string, not a
/// numeric id (the new model has no `RootInput`).
fn root_selector_json(tx: &TxView<'_>) -> Value {
    let mut root = Map::new();
    root.insert("chain_id".into(), Value::from(tx.chain_id));
    root.insert("from".into(), Value::from(tx.from));
    root.insert("to".into(), Value::from(tx.to));
    Value::Object(root)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn swap_view() -> ActionView<'static> {
        ActionView {
            domain: "amm",
            action_tag: Some("swap"),
            venue_name: Some("uniswap_v3"),
        }
    }

    fn tx() -> TxView<'static> {
        TxView {
            chain_id: "eip155:1",
            from: "0x1111111111111111111111111111111111111111",
            to: "0x2222222222222222222222222222222222222222",
        }
    }

    fn manifest(value: serde_json::Value) -> ManifestV2 {
        serde_json::from_value(value).expect("manifest parses")
    }

    #[test]
    fn plans_call_with_resolved_action_and_root_selectors() {
        let m = manifest(json!({
            "id": "swap-usd",
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": "swap" } } },
            "policy_rpc": [{
                "id": "input-usd",
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id",
                    "recipient": "$.action.recipient",
                    "static": "literal"
                },
                "outputs": [{
                    "kind": "context", "field": "totalInputUsd",
                    "type": "Decimal", "from": "$.result.usd"
                }]
            }],
            "custom_context": { "fields": { "totalInputUsd": "decimal" } }
        }));
        let lowered = json!({ "recipient": "0xrecipient", "slippageBp": 50 });

        let calls = plan_policy_rpc_v2(&[m], &swap_view(), &lowered, &tx()).unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id, "swap-usd::input-usd");
        assert_eq!(calls[0].method, "oracle.usd_value");
        assert_eq!(
            calls[0].params,
            json!({
                "chain_id": "eip155:1",
                "recipient": "0xrecipient",
                "static": "literal"
            })
        );
    }

    #[test]
    fn non_matching_trigger_produces_no_calls() {
        let m = manifest(json!({
            "id": "lending-only",
            "schema_version": 2,
            "trigger": { "where": { "action.domain": { "eq": "lending" } } },
            "policy_rpc": [{ "id": "x", "method": "m", "outputs": [] }]
        }));
        let calls = plan_policy_rpc_v2(&[m], &swap_view(), &json!({}), &tx()).unwrap();
        assert!(calls.is_empty());
    }

    #[test]
    fn required_missing_selector_errors_optional_skips() {
        let required = manifest(json!({
            "id": "req",
            "schema_version": 2,
            "trigger": {},
            "policy_rpc": [{
                "id": "c", "method": "m",
                "params": { "x": "$.action.absent" }, "outputs": []
            }]
        }));
        let err = plan_policy_rpc_v2(&[required], &swap_view(), &json!({}), &tx()).unwrap_err();
        assert!(matches!(err, PolicyRpcError::Selector(_)), "{err:?}");

        let optional = manifest(json!({
            "id": "opt",
            "schema_version": 2,
            "trigger": {},
            "policy_rpc": [{
                "id": "c", "method": "m",
                "params": { "x": "$.action.absent" },
                "outputs": [], "optional": true
            }]
        }));
        let calls = plan_policy_rpc_v2(&[optional], &swap_view(), &json!({}), &tx()).unwrap();
        assert!(
            calls.is_empty(),
            "optional call with missing selector is skipped"
        );
    }
}
