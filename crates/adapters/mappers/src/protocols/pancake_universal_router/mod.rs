//! Mappers for PancakeSwap Infinity Universal Router *opcodes*.
//!
//! Pancake UR's `execute(commands, inputs, ...)` outer entrypoint is identical
//! in shape to Uniswap UR's (selectors `0x3593564c` / `0x24856bc3`), but the
//! `0x10` opcode means `INFI_SWAP` (the Pancake Infinity action-stream
//! dispatcher) rather than Uniswap V4's `V4_SWAP`. Same outer payload
//! `(bytes actions, bytes[] params)`, but the inner stream dispatches against
//! [`PANCAKE_INFI_TABLE`](abi_resolver::subdecode::protocols::pancake_infinity::PANCAKE_INFI_TABLE)
//! — Pancake Infinity 6-field `PoolKey` + 6-field `PathKey`, NOT Uniswap V4's
//! 5-field variants (D010).
//!
//! Phase B.3 (P1 mini-round) wires the cross-table builder so the declarative
//! `opcode_stream_dispatch` path emits the inner CL / Bin swap envelopes rather
//! than silently warn-skipping the outer `INFI_SWAP` step (D008 root cause).
//!
//! # Status
//!
//! Currently covers the cross-table builder only — `INFI_SWAP` outer dispatch
//! and the inner swap envelope mapping (CL_SWAP_EXACT_IN(_SINGLE) /
//! CL_SWAP_EXACT_OUT(_SINGLE) and the Bin pool counterparts 0x1c-0x1f). The
//! standalone Pancake Infinity PositionManager liquidity actions
//! (`CL_INCREASE_LIQUIDITY`, `BIN_ADD_LIQUIDITY`, …) reach the declarative
//! pipeline through their own contract-level manifests (the `flat opcode set`
//! dispatcher `pancake_infinity_position_manager`), not through this module.

pub mod infi_swap_builder;

pub use infi_swap_builder::build_pancake_infi_swap_envelopes;
