//! `abi-resolver`
//!
//! Two-tier signature lookup + dynamic calldata decoder for arbitrary EVM
//! transactions. Given `(chain_id, address, calldata)`, find a matching
//! signature (Sourcify by address, then openchain by selector) and decode the
//! standard-ABI portion into named arguments.
//!
//! Non-standard payload encodings packed inside `bytes` arguments (Uniswap V3
//! packed path, Universal Router command streams, Balancer `userData`, …) are
//! covered by the [`subdecode`] module. The first-party adapters in
//! `crates/adapters/*` then layer domain-level mapping on top of the structural
//! decode produced here.
//!
//! Module layout:
//!   - `sourcify`: load + index Sourcify-style ABIs by `(chain, address, selector)`.
//!   - `openchain`: load + index openchain.xyz-style selector → signature dump.
//!   - `decode`: signature + calldata → named argument values.
//!   - `resolver`: tier the lookups (Sourcify first, openchain fallback).
//!   - `subdecode`: parsers for non-standard payloads packed in `bytes` args.

#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]

pub mod bridge;
pub mod decode;
pub mod decoder;
pub mod extract;
pub mod ids;
pub mod in_memory_registry;
pub mod openchain;
pub mod resolver;
pub mod sourcify;
pub mod splitter;
#[cfg(feature = "sqlite")]
pub mod sqlite_index;
pub mod subdecode;

pub use decoder::{
    CallMatchKey, DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder, DecoderError,
    DecoderId, DecoderRegistry,
};
pub use in_memory_registry::{InMemoryDecoderRegistry, InMemoryDecoderRegistryBuilder};
pub use splitter::{
    IdentitySplitter, InMemorySplitterRegistry, InMemorySplitterRegistryBuilder, SplitContext,
    SplitError, Splitter, SplitterRegistry, SubCall, UniversalRouterSplitter,
};
