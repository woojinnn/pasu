//! Cedar policy schema composition.

const CORE_SCHEMA: &str = include_str!("../../../policy-schema/core.cedarschema");
const DEX_SCHEMA: &str = include_str!("../../../policy-schema/actions/dex.cedarschema");
const EIP2612_SCHEMA: &str = include_str!("../../../policy-schema/actions/eip2612.cedarschema");
const EIP712_OTHER_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/eip712_other.cedarschema");
const OTHER_SCHEMA: &str = include_str!("../../../policy-schema/actions/other.cedarschema");
const PERMIT2_SCHEMA: &str = include_str!("../../../policy-schema/actions/permit2.cedarschema");
const SIGNATURE_BASE_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/signature_base.cedarschema");

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
        [
            CORE_SCHEMA,
            DEX_SCHEMA,
            OTHER_SCHEMA,
            SIGNATURE_BASE_SCHEMA,
            PERMIT2_SCHEMA,
            EIP2612_SCHEMA,
            EIP712_OTHER_SCHEMA,
        ]
        .join("\n")
    }
}
