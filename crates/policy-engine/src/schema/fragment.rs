//! Per-action `<Action>CustomContext` Cedar fragments derived from manifests.

use serde::{Deserialize, Serialize};

/// Provenance of one manifest-derived custom context field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomFieldSource {
    /// Destination context field name.
    pub field: String,
    /// Cedar spelling of the field type.
    pub cedar_type: String,
    /// Requirement that contributed this field.
    pub source_requirement_id: String,
    /// Remote method invoked by the requirement.
    pub source_method: String,
    /// Selector rooted at `$.result` that produced the field value.
    pub source_from: String,
    /// Whether the contributing requirement is marked optional.
    pub requirement_optional: bool,
}

/// Cedar text and field provenance for a single action's custom context type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CedarTypeFragment {
    /// Cedar source for the `<Action>CustomContext` type alone.
    pub type_text: String,
    /// Provenance of each field in declaration order.
    pub fields: Vec<CustomFieldSource>,
}

impl CedarTypeFragment {
    /// Build a fragment whose body has no fields.
    #[must_use]
    pub fn empty(action_pascal: &str) -> Self {
        Self {
            type_text: format!("type {action_pascal}CustomContext = {{}};\n"),
            fields: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fragment_has_empty_type_text() {
        let f = CedarTypeFragment::empty("Swap");
        assert!(f.type_text.contains("type SwapCustomContext = {"));
        assert!(f.fields.is_empty());
    }
}
