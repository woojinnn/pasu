//! Cedar schema enriched with manifest-derived custom context fields.

use super::fragment::CustomFieldSource;
use serde::{Deserialize, Serialize};
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
