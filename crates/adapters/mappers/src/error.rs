//! Mapper error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MapError {
    #[error("calldata too short: need {need}, got {got}")]
    TooShort { need: usize, got: usize },

    #[error("wrong selector: got 0x{got}, want 0x{want}")]
    BadSelector { got: String, want: String },

    #[error("ABI decode failed: {0}")]
    AbiDecode(String),

    #[error("field extraction failed: {0}")]
    Extract(#[from] abi_resolver::extract::ExtractError),

    #[error("path has fewer than 2 hops: got {0}")]
    EmptyPath(usize),

    #[error("unsupported command/action: 0x{0:02x}")]
    UnsupportedOpcode(u8),

    #[error("mapping not implemented: {0}")]
    NotImplemented(&'static str),
}
