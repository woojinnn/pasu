//! Cedar policy schema composition.

const CORE_SCHEMA: &str = include_str!("../../../policy-schema/core.cedarschema");
const SWAP_SCHEMA: &str = include_str!("../../../policy-schema/actions/swap.cedarschema");

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
        [CORE_SCHEMA, SWAP_SCHEMA].join("\n")
    }
}
