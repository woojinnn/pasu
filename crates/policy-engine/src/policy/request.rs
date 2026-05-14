//! Cedar evaluation input shape.

use serde_json::Value as JsonValue;

/// Self-contained Cedar evaluation input.
///
/// Action-adapter-driven lowering produces this from a transaction; the
/// policy engine consumes it. The request can be serialized, logged,
/// replayed, and built by hand in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRequest {
    /// Cedar `EntityUid` for the principal — e.g., `Wallet::"0xUser"`.
    pub principal: String,
    /// Cedar `EntityUid` for the action — e.g., `Action::"swap"`.
    pub action: String,
    /// Cedar `EntityUid` for the resource — e.g.,
    /// `Protocol::"0xUniswapV3Router"`.
    pub resource: String,
    /// Cedar entities array (JSON form Cedar accepts).
    pub entities: JsonValue,
    /// Cedar context record (JSON form).
    pub context: JsonValue,
}

impl PolicyRequest {
    /// Construct a policy request from Cedar request components.
    #[must_use]
    pub fn new(
        principal: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        entities: JsonValue,
        context: JsonValue,
    ) -> Self {
        Self {
            principal: principal.into(),
            action: action.into(),
            resource: resource.into(),
            entities,
            context,
        }
    }
}
