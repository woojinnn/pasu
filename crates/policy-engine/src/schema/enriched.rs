//! Cedar schema enriched with manifest-derived custom context fields.

use super::fragment::CustomFieldSource;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composed Cedar schema together with manifest-derived custom field provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrichedSchema {
    /// Final Cedar schema text after merging custom context fragments.
    pub schema_text: String,
    /// Canonical SHA-256 of `schema_text` plus normalized field provenance.
    pub schema_hash: String,
    /// Manifest-contributed fields keyed by action name (`snake_case`).
    pub custom_types_by_action: BTreeMap<String, Vec<CustomFieldSource>>,
}

impl EnrichedSchema {
    /// Build an enriched schema with a canonical hash.
    ///
    /// `per_action` may arrive in any order; fields within each action are
    /// sorted by name before storage. Per D13 the hash is SHA-256 of the
    /// canonical `schema_text` only — provenance is metadata that does not
    /// participate in the hash.
    #[must_use]
    pub fn compute(
        schema_text: impl Into<String>,
        per_action: Vec<(String, Vec<CustomFieldSource>)>,
    ) -> Self {
        let mut map: BTreeMap<String, Vec<CustomFieldSource>> = BTreeMap::new();
        for (action, mut fields) in per_action {
            fields.sort_by(|x, y| x.field.cmp(&y.field));
            map.insert(action, fields);
        }
        let schema_text = schema_text.into();
        let digest = Sha256::digest(schema_text.as_bytes());
        let schema_hash = format!("sha256:{digest:x}");
        Self {
            schema_text,
            schema_hash,
            custom_types_by_action: map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_src(field: &str, ty: &str) -> CustomFieldSource {
        CustomFieldSource {
            field: field.into(),
            cedar_type: ty.into(),
            source_requirement_id: "r".into(),
            source_method: "m".into(),
            source_from: "$.result".into(),
            requirement_optional: false,
        }
    }

    #[test]
    fn schema_hash_is_stable_across_field_insertion_order() {
        let a = EnrichedSchema::compute(
            "base text",
            vec![(
                "swap".into(),
                vec![mk_src("a", "Long"), mk_src("b", "String")],
            )],
        );
        let b = EnrichedSchema::compute(
            "base text",
            vec![(
                "swap".into(),
                vec![mk_src("b", "String"), mk_src("a", "Long")],
            )],
        );
        assert_eq!(a.schema_hash, b.schema_hash);
    }

    #[test]
    fn schema_hash_ignores_provenance_when_schema_text_is_identical() {
        // D13: hash is SHA-256 of the canonical schema text only. Field-level
        // provenance is metadata and does not participate in the hash.
        let a = EnrichedSchema::compute("t", vec![("swap".into(), vec![mk_src("a", "Long")])]);
        let b = EnrichedSchema::compute("t", vec![("swap".into(), vec![mk_src("a", "String")])]);
        assert_eq!(a.schema_hash, b.schema_hash);
    }

    #[test]
    fn schema_hash_differs_when_schema_text_changes() {
        let a = EnrichedSchema::compute("base", vec![]);
        let b = EnrichedSchema::compute("base ", vec![]);
        assert_ne!(a.schema_hash, b.schema_hash);
    }
}
