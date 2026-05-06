//! Uniswap V3 `SwapRouter` adapters for `policy-engine`.
//!
//! ## Module layout
//!
//! Each Uniswap V3 `SwapRouter` function lives in its own module under this
//! crate, with `sol!` declaration, encode/decode helpers, the `Adapter` impl,
//! and unit tests all co-located. Adding a new function (e.g., `exactInput`,
//! `exactOutputSingle`, `multicall`) means: drop a new file next to
//! `exact_input_single.rs`, declare it as `pub mod ...;` here, and re-export
//! its `Adapter_` from this `lib.rs`.
//!
//! ```text
//!   src/
//!     ├── lib.rs                   ← module declarations + flat re-exports
//!     ├── common.rs                ← shared: SWAP_ROUTER_MAINNET, TokenLookup,
//!     │                              shift_decimals, DecodeError
//!     ├── exact_input_single.rs    ← `Adapter_` + `Params` + encode/decode
//!     ├── exact_input.rs
//!     ├── exact_output_single.rs
//!     └── multicall.rs
//! ```
//!
//! The `policy-engine-adapters-bundle` crate stitches each function's
//! `Adapter_` into a single `MockAdapterRegistry` via `default_registry()`.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod common;
pub mod exact_input;
pub mod exact_input_single;
pub mod exact_output;
pub mod exact_output_single;
pub mod multicall;

pub use common::{decode_v3_path, shift_decimals, DecodeError, TokenLookup, SWAP_ROUTER_MAINNET};

// ---- Per-function re-exports ----------------------------------------------
// We re-export each function's `Adapter_` under a distinguishing name so
// callers don't need to write `<func_module>::Adapter_`. These names (and the
// per-function `Params` / `encode` / `decode` / `SELECTOR_*`) form the stable
// surface other crates depend on.

pub use exact_input_single::Adapter_ as UniswapV3ExactInputSingleAdapter;
pub use exact_input_single::{
    decode as decode_exact_input_single, encode as encode_exact_input_single,
    Params as ExactInputSingleParams, SELECTOR as SELECTOR_EXACT_INPUT_SINGLE,
};

pub use exact_input::Adapter_ as UniswapV3ExactInputAdapter;
pub use exact_input::{
    decode as decode_exact_input, encode as encode_exact_input, Params as ExactInputParams,
    SELECTOR as SELECTOR_EXACT_INPUT,
};

pub use exact_output_single::Adapter_ as UniswapV3ExactOutputSingleAdapter;
pub use exact_output_single::{
    decode as decode_exact_output_single, encode as encode_exact_output_single,
    Params as ExactOutputSingleParams, SELECTOR as SELECTOR_EXACT_OUTPUT_SINGLE,
};

pub use exact_output::Adapter_ as UniswapV3ExactOutputAdapter;
pub use exact_output::{
    decode as decode_exact_output, encode as encode_exact_output, Params as ExactOutputParams,
    SELECTOR as SELECTOR_EXACT_OUTPUT,
};

pub use multicall::Adapter_ as UniswapV3MulticallAdapter;
pub use multicall::{
    decode as decode_multicall, encode_deadline as encode_multicall_deadline,
    encode_no_deadline as encode_multicall_no_deadline, Params as MulticallParams,
    SELECTOR_DEADLINE as SELECTOR_MULTICALL_DEADLINE,
    SELECTOR_NO_DEADLINE as SELECTOR_MULTICALL_NO_DEADLINE,
};
