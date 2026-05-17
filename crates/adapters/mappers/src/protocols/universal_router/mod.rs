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
pub mod sweep;
pub mod transfer;
pub mod unwrap_weth;
pub mod v2_swap_exact_in;
pub mod v2_swap_exact_out;
pub mod v3_swap_exact_in;
pub mod v3_swap_exact_out;
pub mod v4_swap;
pub mod wrap_eth;

#[cfg(test)]
mod tests;

pub use sweep::{sweep_mapper_arc, sweep_mapper_key, UrSweepMapper, UR_SWEEP_MAPPER_ID};
pub use transfer::{
    transfer_mapper_arc, transfer_mapper_key, UrTransferMapper, UR_TRANSFER_MAPPER_ID,
};
pub use unwrap_weth::{
    unwrap_weth_mapper_arc, unwrap_weth_mapper_key, UrUnwrapWethMapper, UR_UNWRAP_WETH_MAPPER_ID,
};
pub use v2_swap_exact_in::{
    v2_swap_exact_in_mapper_arc, v2_swap_exact_in_mapper_key, UrV2SwapExactInMapper,
    UR_V2_SWAP_EXACT_IN_MAPPER_ID,
};
pub use v2_swap_exact_out::{
    v2_swap_exact_out_mapper_arc, v2_swap_exact_out_mapper_key, UrV2SwapExactOutMapper,
    UR_V2_SWAP_EXACT_OUT_MAPPER_ID,
};
pub use v3_swap_exact_in::{
    v3_swap_exact_in_mapper_arc, v3_swap_exact_in_mapper_key, UrV3SwapExactInMapper,
    UR_V3_SWAP_EXACT_IN_MAPPER_ID,
};
pub use v3_swap_exact_out::{
    v3_swap_exact_out_mapper_arc, v3_swap_exact_out_mapper_key, UrV3SwapExactOutMapper,
    UR_V3_SWAP_EXACT_OUT_MAPPER_ID,
};
pub use v4_swap::{v4_swap_mapper_arc, v4_swap_mapper_key, UrV4SwapMapper, UR_V4_SWAP_MAPPER_ID};
pub use wrap_eth::{
    wrap_eth_mapper_arc, wrap_eth_mapper_key, UrWrapEthMapper, UR_WRAP_ETH_MAPPER_ID,
};
