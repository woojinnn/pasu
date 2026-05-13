//! Per-protocol sub-format decoders.
//!
//! Each module here owns the parser for one protocol's non-standard payload
//! shape. They depend only on `alloy_primitives` (and `thiserror` for errors);
//! callers with richer error types should map errors at their boundary.

pub mod balancer_v2;
pub mod pancake_infinity;
pub mod pancake_ur;
pub mod safe_multisend;
pub mod uniswap_v3;
pub mod universal_router;
pub mod v4_router;
