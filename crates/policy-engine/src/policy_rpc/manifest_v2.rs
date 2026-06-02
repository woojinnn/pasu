//! Manifest v2 — per-policy bundle metadata (`manifest.json`).
//!
//! A v2 manifest is paired 1:1 with a `policy.cedar` and a generated
//! `policy.cedarschema` (marketplace bundle). It carries three things:
//!
//! 1. a [`Trigger`] — a declarative selector deciding *when* this policy
//!    applies to a decoded [`policy_transition::action::ActionBody`], evaluated
//!    by the host *before* any policy-rpc call (see [`super::trigger`]);
//! 2. a [`PolicyRpcCallSpec`] list — the enrichment calls to run when the
//!    trigger matches (same shape as the v1 `requires[]`, minus the per-call
//!    `when` gate, which the manifest-wide trigger subsumes);
//! 3. a [`CustomContext`] — the extra Cedar context fields this policy's
//!    `.cedarschema` declares, each fed by one [`PolicyRpcCallSpec`] output.
//!
//! This is the active per-policy manifest shape for the ActionBody pipeline.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ContextProjection, PolicyRpcError};

/// The manifest schema version this module parses. A manifest whose
/// `schema_version` differs is rejected by [`ManifestV2::validate`].
pub const MANIFEST_V2_SCHEMA_VERSION: u64 = 2;

/// A per-policy v2 manifest (`manifest.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestV2 {
    /// Stable bundle id (also the policy id and the bundle directory name).
    pub id: String,
    /// Manifest schema version. Must equal [`MANIFEST_V2_SCHEMA_VERSION`].
    pub schema_version: u64,
    /// Declarative selector deciding when this policy applies.
    #[serde(default)]
    pub trigger: Trigger,
    /// Enrichment calls executed when the trigger matches.
    #[serde(default)]
    pub policy_rpc: Vec<PolicyRpcCallSpec>,
    /// Extra Cedar context fields this policy declares.
    #[serde(default)]
    pub custom_context: CustomContext,
}

/// A declarative trigger: a [`TriggerScope`] plus a conjunction of per-field
/// constraints. An empty [`Trigger::where_`] matches every action.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Trigger {
    /// Which action a Multicall feeds to the evaluator (inner children vs the
    /// outer batch). Defaults to [`TriggerScope::Inner`].
    #[serde(default)]
    pub scope: TriggerScope,
    /// Field → constraint map. All entries must hold (implicit AND). Empty →
    /// always matches. Named `where` on the wire; `where_` in Rust (keyword).
    #[serde(default, rename = "where")]
    pub where_: BTreeMap<TriggerField, TriggerConstraint>,
}

/// Whether a Multicall is matched per inner child or once for the outer batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerScope {
    /// Evaluate the trigger once per inner child action (default). A
    /// non-Multicall action is its own single "inner" action.
    #[default]
    Inner,
    /// Evaluate the trigger once against the outer action (a Multicall's
    /// domain is `"multicall"`).
    Outer,
}

/// The closed set of top-level fields a trigger may match on. Kept deliberately
/// shallow (no nested `action.params.*`) so triggers are cheap to evaluate and
/// stable against deep schema drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TriggerField {
    /// `ActionBody` domain tag: `token` / `amm` / `lending` / `airdrop` /
    /// `launchpad` / `perp` / `multicall` / `unknown`.
    #[serde(rename = "action.domain")]
    ActionDomain,
    /// Inner action tag (e.g. `swap`, `supply`). Absent for multicall/unknown.
    #[serde(rename = "action.tag")]
    ActionTag,
    /// Venue name (e.g. `uniswap_v3`, `aave_v3`). Absent when the action has
    /// no venue (token/airdrop/launchpad).
    #[serde(rename = "action.venue")]
    ActionVenue,
    /// Transaction chain id (CAIP-2, e.g. `eip155:1`).
    #[serde(rename = "tx.chain_id")]
    TxChainId,
    /// Transaction submitter address.
    #[serde(rename = "tx.from")]
    TxFrom,
    /// Transaction target address.
    #[serde(rename = "tx.to")]
    TxTo,
}

/// A single-field constraint. String comparison only — numeric/logical
/// predicates belong in the Cedar policy body, not the trigger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerConstraint {
    /// Field equals this value.
    Eq(String),
    /// Field does not equal this value (true when the field is absent).
    Ne(String),
    /// Field is one of this set.
    In(Vec<String>),
    /// Field is none of this set (true when the field is absent).
    Nin(Vec<String>),
}

/// One enrichment call. Identical to the legacy `Requirement` minus the
/// per-call `when` gate (the manifest-wide [`Trigger`] subsumes it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRpcCallSpec {
    /// Call id, unique within this manifest.
    pub id: String,
    /// Opaque remote method name (e.g. `oracle.usd_value`).
    pub method: String,
    /// Parameter template. String values beginning with `$.` are selectors.
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    /// Output projection rules, rooted at `$.result`.
    #[serde(default)]
    pub outputs: Vec<ContextProjection>,
    /// When true, a missing param selector skips this call instead of failing.
    #[serde(default)]
    pub optional: bool,
}

/// The custom Cedar context fields this policy declares.
///
/// A map of `field_name → Cedar type spelling` (e.g.
/// `"totalInputUsd" → "decimal"`). Injected into the action's
/// `<Action>CustomContext` stub during per-policy schema synthesis. Use only
/// types the synthesized schema defines — Cedar primitives / `decimal` /
/// `Set<...>` — not the removed legacy records (`UsdValuation`/`WindowStats`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CustomContext {
    /// Declared field → Cedar type spelling.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

impl ManifestV2 {
    /// Validate structural invariants:
    ///
    /// 1. `schema_version` equals [`MANIFEST_V2_SCHEMA_VERSION`];
    /// 2. `policy_rpc[].id` values are unique;
    /// 3. every [`CustomContext`] field is fed by some `policy_rpc[].outputs[]`
    ///    (1:1 — no orphan declared field, no silent stale field).
    ///
    /// # Errors
    ///
    /// Returns [`PolicyRpcError::InvalidManifest`] on any violation.
    pub fn validate(&self) -> Result<(), PolicyRpcError> {
        if self.schema_version != MANIFEST_V2_SCHEMA_VERSION {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "manifest `{}` has schema_version {} (expected {MANIFEST_V2_SCHEMA_VERSION})",
                self.id, self.schema_version
            )));
        }

        let mut call_ids = HashSet::new();
        let mut produced_fields = HashSet::new();
        for call in &self.policy_rpc {
            if !call_ids.insert(call.id.as_str()) {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "manifest `{}` has duplicate policy_rpc id `{}`",
                    self.id, call.id
                )));
            }
            for output in &call.outputs {
                produced_fields.insert(output.field.as_str());
            }
        }

        for field in self.custom_context.fields.keys() {
            if !produced_fields.contains(field.as_str()) {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "manifest `{}` declares custom_context field `{field}` with no \
                     producing policy_rpc output",
                    self.id
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn large_swap_manifest() -> Value {
        json!({
            "id": "large-swap-usd-warning",
            "schema_version": 2,
            "trigger": {
                "where": { "action.tag": { "eq": "swap" } }
            },
            "policy_rpc": [{
                "id": "total-input-usd",
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id"
                },
                "outputs": [{
                    "kind": "context",
                    "field": "totalInputUsd",
                    "type": "Decimal",
                    "from": "$.result.usd",
                    "required": false
                }],
                "optional": true
            }],
            "custom_context": {
                "fields": { "totalInputUsd": "decimal" }
            }
        })
    }

    #[test]
    fn parses_and_round_trips_large_swap_manifest() {
        let manifest: ManifestV2 = serde_json::from_value(large_swap_manifest()).unwrap();
        assert_eq!(manifest.id, "large-swap-usd-warning");
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(
            manifest.trigger.where_.get(&TriggerField::ActionTag),
            Some(&TriggerConstraint::Eq("swap".to_owned()))
        );
        assert_eq!(manifest.policy_rpc.len(), 1);
        assert_eq!(manifest.policy_rpc[0].method, "oracle.usd_value");
        assert_eq!(
            manifest
                .custom_context
                .fields
                .get("totalInputUsd")
                .map(String::as_str),
            Some("decimal")
        );
        manifest.validate().expect("valid manifest");

        // Round-trip through serde to confirm wire stability.
        let back: ManifestV2 =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        assert_eq!(manifest, back);
    }

    #[test]
    fn empty_trigger_default_and_no_rpc_is_valid() {
        let manifest: ManifestV2 = serde_json::from_value(json!({
            "id": "always-audit",
            "schema_version": 2
        }))
        .unwrap();
        assert!(manifest.trigger.where_.is_empty());
        assert_eq!(manifest.trigger.scope, TriggerScope::Inner);
        assert!(manifest.policy_rpc.is_empty());
        manifest.validate().expect("empty manifest is valid");
    }

    #[test]
    fn outer_scope_and_in_constraint_parse() {
        let manifest: ManifestV2 = serde_json::from_value(json!({
            "id": "multicall-bundle-warn",
            "schema_version": 2,
            "trigger": {
                "scope": "outer",
                "where": { "action.domain": { "eq": "multicall" } }
            }
        }))
        .unwrap();
        assert_eq!(manifest.trigger.scope, TriggerScope::Outer);

        let venue_in: ManifestV2 = serde_json::from_value(json!({
            "id": "uniswap-v3-only",
            "schema_version": 2,
            "trigger": {
                "where": {
                    "action.tag": { "eq": "swap" },
                    "action.venue": { "in": ["uniswap_v3", "uniswap_v4"] }
                }
            }
        }))
        .unwrap();
        assert_eq!(
            venue_in.trigger.where_.get(&TriggerField::ActionVenue),
            Some(&TriggerConstraint::In(vec![
                "uniswap_v3".to_owned(),
                "uniswap_v4".to_owned()
            ]))
        );
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let manifest: ManifestV2 = serde_json::from_value(json!({
            "id": "v1-shaped",
            "schema_version": 1
        }))
        .unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(err.to_string().contains("schema_version 1"));
    }

    #[test]
    fn rejects_duplicate_policy_rpc_id() {
        let manifest: ManifestV2 = serde_json::from_value(json!({
            "id": "dup",
            "schema_version": 2,
            "policy_rpc": [
                { "id": "x", "method": "a", "outputs": [] },
                { "id": "x", "method": "b", "outputs": [] }
            ]
        }))
        .unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate policy_rpc id `x`"));
    }

    #[test]
    fn rejects_orphan_custom_context_field() {
        let manifest: ManifestV2 = serde_json::from_value(json!({
            "id": "orphan",
            "schema_version": 2,
            "policy_rpc": [{ "id": "c", "method": "m", "outputs": [] }],
            "custom_context": { "fields": { "ghost": "Long" } }
        }))
        .unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(err.to_string().contains("custom_context field `ghost`"));
    }
}
