//! Per-command mappers for Universal Router. Each module corresponds to one
//! `Commands.sol` opcode (post `& 0x7F` to strip the `allowRevert` flag).
//!
//! | Opcode | Module | Output action |
//! |---|---|---|
//! | 0x00 | `v3_swap_in`     | `SwapAction` (exact_in, V3-style) |
//! | 0x01 | `v3_swap_out`    | `SwapAction` (exact_out, V3-style) |
//! | 0x08 | `v2_swap_in`     | `SwapAction` (exact_in, V2-style) |
//! | 0x09 | `v2_swap_out`    | `SwapAction` (exact_out, V2-style) |
//! | 0x10 | `v4_swap`        | `Vec<SwapAction>` (V4 inner dispatch) |
//! | 0x0b | `wrap_eth`       | `WrapAction`   |
//! | 0x0c | `unwrap_weth`    | `UnwrapAction` |
//! | 0x0a | `permit2_permit` | (handled by signature adapter; emit Approve or no-op) |
//!
//! Rare/ignored opcodes (0x02/0x03/0x04/0x05/0x06/0x0d/0x21) can be added
//! later — they don't represent user-intent actions in policy terms.

pub mod permit2_permit;
pub mod unwrap_weth;
pub mod v2_swap_in;
pub mod v2_swap_out;
pub mod v3_swap_in;
pub mod v3_swap_out;
pub mod v4_swap;
pub mod wrap_eth;
