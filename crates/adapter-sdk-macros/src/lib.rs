//! `#[adapter]` proc-macro. Place it on a unit-struct or named struct that
//! implements one or more of `Decoder`, `CallAdapter`, `SignAdapter`.
//!
//! Example:
//!
//! ```ignore
//! use adapter_sdk::prelude::*;
//!
//! #[adapter(
//!     name        = "erc20-transfer",
//!     version     = "0.1.0",
//!     description = "ERC-20 transfer canary",
//!     applies_to  = [{ chain: 1, address: "0xa0b8...eb48" }],
//!     capabilities = [decoder, call_adapter],
//! )]
//! struct Erc20Transfer;
//! ```

mod codegen;
mod parse;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn adapter(args: TokenStream, input: TokenStream) -> TokenStream {
    match parse::parse_args(args.into()) {
        Ok(args) => codegen::expand(args, input.into()).into(),
        Err(err) => err.to_compile_error().into(),
    }
}
