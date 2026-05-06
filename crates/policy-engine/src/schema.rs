//! Cedar policy schema composition.

const CORE_SCHEMA: &str = include_str!("../../../policy-schema/core.cedarschema");
const DEX_SCHEMA: &str = include_str!("../../../policy-schema/actions/dex.cedarschema");
const OTHER_SCHEMA: &str = include_str!("../../../policy-schema/actions/other.cedarschema");

/// Composes the shipped core and action Cedar schemas.
#[derive(Debug, Default, Clone)]
pub struct PolicySchemaComposer;

impl PolicySchemaComposer {
    /// Construct a schema composer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Return the concatenated Cedar schema text.
    #[must_use]
    pub fn compose(&self) -> String {
        [CORE_SCHEMA, DEX_SCHEMA, OTHER_SCHEMA].join("\n")
    }
}
