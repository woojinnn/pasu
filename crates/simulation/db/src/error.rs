//! DB 에러 타입.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json encode/decode error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("migration failed at {step}: {reason}")]
    Migration { step: String, reason: String },

    #[error("entity not found: {kind}={id}")]
    NotFound { kind: &'static str, id: String },

    #[error("invariant violation: {0}")]
    Invariant(String),
}

pub type DbResult<T> = Result<T, DbError>;
