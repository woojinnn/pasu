//! Uniswap Universal Router adapters.
//!
//! The Universal Router is command-based: one outer `execute(...)` call can
//! contain v2, v3, v4, payment, Permit2, and nested sub-plan commands. The
//! adapter extracts supported swap commands into leaf `SwapAction`s so existing
//! swap policies apply unchanged.

pub mod command_decode;
pub mod commands;
pub mod common;
pub mod execute;
pub mod execute_deadline;
pub mod v4_actions;

pub use execute::{decode, encode_execute, Adapter_, DecodeError, Params, SELECTOR_EXECUTE};
pub use execute_deadline::{
    decode as decode_execute_deadline, encode_execute_deadline, SELECTOR_EXECUTE_DEADLINE,
};
pub use v4_actions::{
    PathKey, PoolKey, V4ExactInputParams, V4ExactInputSingleParams, V4ExactOutputParams,
    V4ExactOutputSingleParams,
};
