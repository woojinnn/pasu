//! `abi-resolver`
//!
//! Two-tier signature lookup + dynamic calldata decoder for arbitrary EVM
//! transactions. Given `(chain_id, address, calldata)`, find a matching
//! signature (Sourcify by address, then openchain by selector) and decode the
//! standard-ABI portion into named arguments.
//!
//! Non-standard payload encodings (Uniswap V3 packed path, Universal Router
//! command streams, etc.) are out of scope here — this crate only covers what
//! `alloy_dyn_abi` can decode generically. The first-party adapters in
//! `crates/adapters/*` remain the precise path for those.
//!
//! Module layout:
//!   - `sourcify`: load + index Sourcify-style ABIs by `(chain, address, selector)`.
//!   - `openchain`: load + index openchain.xyz-style selector → signature dump.
//!   - `decode`: signature + calldata → named argument values.
//!   - `resolver`: tier the lookups (Sourcify first, openchain fallback).

pub mod decode;
pub mod openchain;
pub mod resolver;
pub mod sourcify;
pub mod sqlite_index;
