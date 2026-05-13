//! Uniswap V4 Router action mappers — 4 swap actions.
//!
//! These are NOT independent functions — they are sub-actions inside the
//! Universal Router `V4_SWAP` command (`0x10`). The UR command's `inputs[i]`
//! is `abi.encode(bytes actions, bytes[] params)`; each byte in `actions` is
//! one of the IDs below and `params[i]` is the matching struct.
//!
//! | Action ID | Name | Param struct |
//! |---|---|---|
//! | 0x06 | SWAP_EXACT_IN_SINGLE  | `IV4Router.ExactInputSingleParams`  |
//! | 0x07 | SWAP_EXACT_IN         | `IV4Router.ExactInputParams`        |
//! | 0x08 | SWAP_EXACT_OUT_SINGLE | `IV4Router.ExactOutputSingleParams` |
//! | 0x09 | SWAP_EXACT_OUT        | `IV4Router.ExactOutputParams`       |
//!
//! `PoolKey { currency0, currency1, fee, tickSpacing, hooks }` →
//! `(tokenIn, tokenOut)` decided by `zeroForOne`; `feeBps = fee / 100`
//! (or omit when the pool uses dynamic fees, indicated by `fee == 0x800000`).

pub mod common;
pub mod swap_exact_in;
pub mod swap_exact_in_single;
pub mod swap_exact_out;
pub mod swap_exact_out_single;
