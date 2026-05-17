//! Mappers for Uniswap Universal Router *opcodes*.
//!
//! Plain contract functions (V3 `exactInput`, ERC20 `approve`, …) live in
//! their own sibling modules. This module is for opcodes that only exist
//! inside UR's `execute(commands, inputs)` opcode stream — they have no
//! standalone selector and no Sourcify ABI. The
//! [`UniversalRouterSplitter`](abi_resolver::UniversalRouterSplitter)
//! pre-decodes each opcode step and tags it with a synthetic decoder id
//! (see `abi_resolver::ids::UR_*_DECODER_ID`); these mappers consume those
//! pre-decoded calls.
//!
//! # Status
//!
//! Phase 4a (this module's first iteration) covers only `WRAP_ETH` and
//! `UNWRAP_WETH`. The other UR opcodes (V2/V3/V4 swap, SWEEP, TRANSFER,
//! PERMIT2_*) land in subsequent phases.

mod common;
pub mod unwrap_weth;
pub mod wrap_eth;

#[cfg(test)]
mod tests;

pub use unwrap_weth::{
    unwrap_weth_mapper_arc, unwrap_weth_mapper_key, UrUnwrapWethMapper, UR_UNWRAP_WETH_MAPPER_ID,
};
pub use wrap_eth::{
    wrap_eth_mapper_arc, wrap_eth_mapper_key, UrWrapEthMapper, UR_WRAP_ETH_MAPPER_ID,
};
