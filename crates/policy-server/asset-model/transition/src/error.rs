//! Reducer error types.

use thiserror::Error;

/// Errors that can be produced while reducing an `Action` against a `WalletState`.
#[derive(Debug, Error)]
pub enum ReducerError {
    /// The action variant is not recognised by any reducer.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// A required field on the action was absent.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// Referenced token holding was not found in the wallet state.
    #[error("token not found: {0:?}")]
    TokenNotFound(policy_state::TokenKey),

    /// Referenced position id was not found in the wallet state.
    #[error("position not found: {0}")]
    PositionNotFound(String),

    /// An internal invariant of the reducer was violated.
    #[error("invariant violation: {0}")]
    Invariant(String),

    /// The action's protocol/venue is not implemented for that action kind.
    #[error("unsupported protocol: {protocol} for action {action}")]
    UnsupportedProtocol {
        /// Action name (e.g. `"swap"`, `"supply"`).
        action: String,
        /// Protocol name (e.g. `"curve_v2"`, `"aave_v3"`).
        protocol: String,
    },
}

/// Convenience `Result` alias for reducer operations.
pub type ReducerResult<T> = Result<T, ReducerError>;
