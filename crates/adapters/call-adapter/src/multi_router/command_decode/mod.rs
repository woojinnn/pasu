//! Per-opcode inner-input decoders for Universal Router commands.
//!
//! Each module here decodes one opcode's `inputs[i]` ABI tuple and produces
//! either one `ActionEnvelope` (most opcodes) or several (`v4_swap` which
//! dispatches against a nested V4 action stream).

pub(super) mod sweep;
pub(super) mod transfer;
pub(super) mod unwrap_weth;
pub(super) mod v2_swap_exact_in;
pub(super) mod v2_swap_exact_out;
pub(super) mod v3_swap_exact_in;
pub(super) mod v3_swap_exact_out;
pub(super) mod v4_swap;
pub(super) mod wrap_eth;
