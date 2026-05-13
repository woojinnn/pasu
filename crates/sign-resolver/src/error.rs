use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignResolveError {
    #[error("unsupported sign method: {0}")]
    UnsupportedMethod(String),
    #[error("params must be a JSON array")]
    ParamsNotArray,
    #[error("missing required param at index {0}")]
    MissingParam(usize),
    #[error("invalid signer address: {0}")]
    InvalidSigner(String),
    #[error("invalid typed data: {0}")]
    InvalidTypedData(String),
}
