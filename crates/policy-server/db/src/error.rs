use thiserror::Error;

/// Errors returned by policy-server persistence code.
#[derive(Debug, Error)]
pub enum DbError {
    /// JSON serialization or deserialization failed.
    #[error("json encode/decode error: {0}")]
    Json(#[from] serde_json::Error),

    /// A schema migration failed at the named step.
    #[error("migration failed at {step}: {reason}")]
    Migration {
        /// Migration step or filename.
        step: String,
        /// Failure reason.
        reason: String,
    },

    /// A requested entity was absent.
    #[error("entity not found: {kind}={id}")]
    NotFound {
        /// Entity kind.
        kind: &'static str,
        /// Entity identifier.
        id: String,
    },

    /// A database invariant was violated.
    #[error("invariant violation: {0}")]
    Invariant(String),
}

/// Result alias for database operations.
pub type DbResult<T> = Result<T, DbError>;

/// `DbError` bubbles into `StoreError::Backend` for callers that operate
/// against the `WalletStore` trait — the trait stays narrow and DB
/// errors keep their full Display string in the message body.
impl From<DbError> for policy_state::store::StoreError {
    fn from(e: DbError) -> Self {
        Self::Backend(e.to_string())
    }
}
