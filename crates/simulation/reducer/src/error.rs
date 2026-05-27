//! Reducer 에러 타입.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReducerError {
    #[error("unknown action: {0}")]
    UnknownAction(String),

    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("token not found: {0:?}")]
    TokenNotFound(simulation_state::TokenKey),

    #[error("position not found: {0}")]
    PositionNotFound(String),

    #[error("invariant violation: {0}")]
    Invariant(String),

    #[error("unsupported protocol: {protocol} for action {action}")]
    UnsupportedProtocol {
        action: String,
        protocol: String,
    },
}

pub type ReducerResult<T> = Result<T, ReducerError>;
