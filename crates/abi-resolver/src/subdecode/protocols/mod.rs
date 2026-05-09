//! Per-protocol sub-format decoders.
//!
//! Each module here owns the parser for one protocol's non-standard payload
//! shape. They depend only on `alloy_primitives` (and `thiserror` for errors);
//! callers with richer error types should map errors at their boundary.

pub mod uniswap_v3;
pub mod universal_router;
pub mod v4_router;
