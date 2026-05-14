//! Error type for the policy engine.

use thiserror::Error;

/// Error produced while parsing, validating, or evaluating policies.
#[derive(Debug, Error)]
pub enum PolicyError {
    /// Cedar policy parse failure.
    #[error("failed to parse Cedar policy: {0}")]
    Parse(String),
    /// Cedar schema parse failure.
    #[error("failed to parse Cedar schema: {0}")]
    Schema(String),
    /// Cedar schema validation failure.
    #[error("failed to validate Cedar policy set against schema: {0}")]
    Validation(String),
    /// Cedar request construction failure.
    #[error("failed to build Cedar request: {0}")]
    Request(String),
    /// Cedar context construction failure.
    #[error("failed to build Cedar context: {0}")]
    Context(String),
    /// Cedar entities construction failure.
    #[error("failed to build Cedar entities: {0}")]
    Entities(String),
    /// Cedar entity uid construction failure.
    #[error("invalid entity uid: {0}")]
    EntityUid(String),
    /// Semantic action lowering failure before Cedar request construction.
    #[error("lowering failed: {0}")]
    Lowering(String),
}
