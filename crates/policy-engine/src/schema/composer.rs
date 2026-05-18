//! Compose the shipped cedarschemas with per-action manifest fragments.

use super::action_name::{snake_to_pascal, REGISTERED_ACTIONS};
use super::enriched::EnrichedSchema;
use super::fragment::{CedarTypeFragment, CustomFieldSource};
use super::manifest_fragment::manifest_to_cedarschema;
use crate::policy_rpc::{PolicyManifest, PolicyRpcError};
use std::collections::BTreeMap;

/// Compose the bundled base cedarschemas with manifest-derived custom context
/// fragments.
///
/// # Errors
///
/// Returns an error when any manifest fails the validation rules of
/// [`manifest_to_cedarschema`].
// TODO(phase-2): callers will use this once the shipped cedarschemas declare
//   the empty `<Action>CustomContext = {};` stub for every registered action.
pub fn compose_enriched(
    manifests: &BTreeMap<String, PolicyManifest>,
) -> Result<EnrichedSchema, PolicyRpcError> {
    let base_text = super::base_schema_text();
    compose_enriched_with_base(&base_text, manifests)
}

/// Variant of [`compose_enriched`] that takes the base schema text directly.
///
/// Splitting this out lets tests verify the merge logic against a hand-built
/// base that already contains the Phase 2 `<Action>CustomContext = {};` stubs.
///
/// # Errors
///
/// Returns an error when any manifest fails the validation rules of
/// [`manifest_to_cedarschema`].
pub fn compose_enriched_with_base(
    base_text: &str,
    manifests: &BTreeMap<String, PolicyManifest>,
) -> Result<EnrichedSchema, PolicyRpcError> {
    let mut fragments: BTreeMap<String, CedarTypeFragment> = BTreeMap::new();
    for (action, manifest) in manifests {
        let f = manifest_to_cedarschema(action, manifest)?;
        fragments.insert(action.clone(), f);
    }

    let mut text = base_text.to_owned();
    for action in REGISTERED_ACTIONS {
        let pascal = snake_to_pascal(action);
        let stub = format!("type {pascal}CustomContext = {{}};\n");
        if let Some(f) = fragments.get(*action) {
            text = text.replace(&stub, &f.type_text);
        }
    }

    let per_action: Vec<(String, Vec<CustomFieldSource>)> =
        fragments.into_iter().map(|(k, v)| (k, v.fields)).collect();

    Ok(EnrichedSchema::compute(text, per_action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_rpc::{
        ContextProjection, PolicyManifest, ProjectionType, Requirement, RequirementWhen,
    };

    fn swap_manifest_with_total_input_usd() -> PolicyManifest {
        PolicyManifest {
            id: "test::swap".into(),
            schema_version: 1,
            requires: vec![Requirement {
                id: "req1".into(),
                when: RequirementWhen {
                    action: "swap".into(),
                },
                method: "oracle.usd_value".into(),
                params: BTreeMap::default(),
                outputs: vec![ContextProjection {
                    kind: "context".into(),
                    field: "totalInputUsd".into(),
                    type_name: ProjectionType::UsdValuation,
                    from: "$.result".into(),
                    required: false,
                }],
                optional: true,
            }],
            context_extensions: BTreeMap::default(),
        }
    }

    /// Hand-built base text simulating Phase 2's post-rewrite swap schema.
    fn phase2_swap_base() -> String {
        // The empty stub line below is what Phase 2 will add. The composer
        // replaces it with the manifest-derived custom type body.
        String::from(
            "type SwapContext = {\n  swapMode: String,\n  custom?: SwapCustomContext,\n};\n\
             type SwapCustomContext = {};\n",
        )
    }

    #[test]
    fn composer_inserts_custom_type_text_into_action_schema() {
        let m = swap_manifest_with_total_input_usd();
        let manifests = BTreeMap::from([("swap".to_owned(), m)]);
        let enriched = compose_enriched_with_base(&phase2_swap_base(), &manifests).expect("ok");
        assert!(enriched.schema_text.contains("type SwapCustomContext = {"));
        assert!(enriched
            .schema_text
            .contains("totalInputUsd?: UsdValuation"));
        assert!(enriched.schema_text.contains("custom?: SwapCustomContext"));
        assert!(!enriched
            .schema_text
            .contains("type SwapCustomContext = {};"));
        let swap_fields = enriched.custom_types_by_action.get("swap").unwrap();
        assert_eq!(swap_fields.len(), 1);
        assert_eq!(swap_fields[0].field, "totalInputUsd");
    }

    #[test]
    fn composer_leaves_unmentioned_action_stubs_alone() {
        let m = swap_manifest_with_total_input_usd();
        let manifests = BTreeMap::from([("swap".to_owned(), m)]);
        let base =
            String::from("type SwapCustomContext = {};\ntype AddLiquidityCustomContext = {};\n");
        let enriched = compose_enriched_with_base(&base, &manifests).unwrap();
        assert!(enriched
            .schema_text
            .contains("type AddLiquidityCustomContext = {};"));
        assert!(!enriched
            .schema_text
            .contains("type SwapCustomContext = {};"));
    }
}
