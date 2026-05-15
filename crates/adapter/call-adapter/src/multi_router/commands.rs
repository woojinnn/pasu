//! Universal-Router-style command-stream dispatcher.
//!
//! Walks the `(commands, inputs)` pair produced by [`super::execute::decode_outer_call`]
//! and emits an `ActionEnvelope` per recognized opcode. Per-opcode decoding lives
//! in [`super::command_decode`].
//!
//! # Fork support
//!
//! UR-style routers from different DEX families (Uniswap, PancakeSwap, …) share
//! the outer ABI shape but use **different opcode tables** — the bit mask, the
//! numeric values per opcode, even which opcodes exist all vary. To support
//! forks without copying the dispatcher code, we abstract over the opcode
//! mapping with [`OpcodeConstants`]: each fork supplies its own `classify`
//! function turning a raw byte into a fork-agnostic [`OpcodeKind`]. The
//! dispatcher then matches on the kind, not the number.
//!
//! # Safety guards
//!
//! - `MAX_DEPTH` (4): caps `EXECUTE_SUB_PLAN` recursion depth.
//! - `MAX_COMMANDS` (64): caps the number of commands per stream — bounds
//!   memory and CPU use against malicious or buggy calldata.
//! - Length mismatch (`commands.len() != inputs.len()`) → error.
//! - Unknown opcodes → error (no silent skips). Forks opt out cleanly by
//!   classifying recognised non-swap opcodes as [`OpcodeKind::Ignored`].

use alloy_sol_types::{sol, SolType};
use policy_engine::action::{ActionEnvelope, Validity};

use crate::{AdapterError, CallContext};

use super::command_decode;

// ABI shape of an `EXECUTE_SUB_PLAN` input: a nested `(commands, inputs[])`
// tuple. The dispatcher decodes this and recurses back into `expand_commands`
// with `depth + 1`.
type SubPlanInput = sol! { (bytes, bytes[]) };

/// Maximum number of commands per stream (protects against memory exhaustion).
pub(super) const MAX_COMMANDS: usize = 64;

/// Maximum sub-plan recursion depth (protects against stack exhaustion via
/// `EXECUTE_SUB_PLAN` cycles).
pub(super) const MAX_DEPTH: usize = 4;

/// Fork-agnostic opcode classification. The numeric opcode value is fork-
/// specific; this enum is what the dispatcher actually matches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpcodeKind {
    V3SwapExactIn,
    V3SwapExactOut,
    V2SwapExactIn,
    V2SwapExactOut,
    WrapEth,
    UnwrapWeth,
    V4Swap,
    ExecuteSubPlan,
    /// Recognised but intentionally not surfaced as an envelope (Permit2 family,
    /// SWEEP/TRANSFER/PAY_PORTION, position-manager calls, balance checks).
    Ignored,
    /// Not in the fork's opcode table — dispatcher returns an error.
    Unknown,
}

/// Per-fork opcode mapping. The dispatcher (`expand_commands`) is agnostic;
/// each fork supplies one of these to plug into it.
#[derive(Debug, Clone, Copy)]
pub struct OpcodeConstants {
    /// Mask applied to the raw command byte to extract the opcode bits.
    /// Uniswap UR uses `0x7f` (high bit = allowRevert); Pancake UR uses
    /// `0x3f` (top two bits reserved).
    pub mask: u8,
    /// Map a masked opcode value to a fork-agnostic kind.
    pub classify: fn(u8) -> OpcodeKind,
}

impl OpcodeConstants {
    /// Uniswap Universal Router opcode table.
    pub const UNISWAP_UR: Self = Self {
        mask: 0x7f,
        classify: classify_uniswap_ur,
    };
}

fn classify_uniswap_ur(opcode: u8) -> OpcodeKind {
    match opcode {
        0x00 => OpcodeKind::V3SwapExactIn,
        0x01 => OpcodeKind::V3SwapExactOut,
        0x08 => OpcodeKind::V2SwapExactIn,
        0x09 => OpcodeKind::V2SwapExactOut,
        0x0b => OpcodeKind::WrapEth,
        0x0c => OpcodeKind::UnwrapWeth,
        0x10 => OpcodeKind::V4Swap,
        0x21 => OpcodeKind::ExecuteSubPlan,
        // Recognised non-swap commands — intentionally ignored:
        //
        //   Permit2 family (0x02/0x03/0x0a/0x0d): permit semantics are gated
        //   on the *sign* side by `sign_resolver::adapters::permit2`. Inside
        //   UR these commands just replay the same permit (or `transferFrom`)
        //   the user already authorised, so we emit no extra envelope here.
        //
        //   Settlement/utility (0x04 SWEEP, 0x05 TRANSFER, 0x06 PAY_PORTION,
        //   0x07 PAY_PORTION_FULL_PRECISION, 0x0e BALANCE_CHECK_ERC20):
        //   plumbing around the swap result; absorbed by the merge step.
        //
        //   V3/V4 position manager (0x11–0x14): liquidity-position operations
        //   outside the current swap policy scope.
        0x02 | 0x03 | 0x04 | 0x05 | 0x06 | 0x07 | 0x0a | 0x0d | 0x0e | 0x11 | 0x12
        | 0x13 | 0x14 => OpcodeKind::Ignored,
        _ => OpcodeKind::Unknown,
    }
}

/// Walk a UR-style command stream, dispatching each opcode to its decoder and
/// collecting the resulting envelopes.
///
/// `depth` tracks `EXECUTE_SUB_PLAN` recursion (top-level call passes `0`).
/// `oc` carries the fork-specific opcode mapping (see [`OpcodeConstants`]).
pub(super) fn expand_commands(
    ctx: &CallContext<'_>,
    commands: &[u8],
    inputs: &[Vec<u8>],
    validity: Option<Validity>,
    depth: usize,
    oc: &OpcodeConstants,
) -> Result<Vec<ActionEnvelope>, AdapterError> {
    if depth > MAX_DEPTH {
        return Err(AdapterError::Invalid(format!(
            "Universal Router sub-plan depth exceeds max {MAX_DEPTH}"
        )));
    }
    if commands.len() != inputs.len() {
        return Err(AdapterError::Invalid(format!(
            "Universal Router length mismatch: {} commands, {} inputs",
            commands.len(),
            inputs.len()
        )));
    }
    if commands.len() > MAX_COMMANDS {
        return Err(AdapterError::Invalid(format!(
            "Universal Router command count {} exceeds max {MAX_COMMANDS}",
            commands.len()
        )));
    }

    let mut envelopes = Vec::new();

    for (index, raw_opcode) in commands.iter().copied().enumerate() {
        let input = &inputs[index];
        let opcode = raw_opcode & oc.mask;
        let kind = (oc.classify)(opcode);
        match kind {
            OpcodeKind::V3SwapExactIn => {
                envelopes.push(command_decode::v3_swap_exact_in::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            OpcodeKind::V3SwapExactOut => {
                envelopes.push(command_decode::v3_swap_exact_out::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            OpcodeKind::V2SwapExactIn => {
                envelopes.push(command_decode::v2_swap_exact_in::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            OpcodeKind::V2SwapExactOut => {
                envelopes.push(command_decode::v2_swap_exact_out::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            OpcodeKind::WrapEth => {
                envelopes.push(command_decode::wrap_eth::decode(ctx, input)?);
            }
            OpcodeKind::UnwrapWeth => {
                envelopes.push(command_decode::unwrap_weth::decode(ctx, input)?);
            }
            OpcodeKind::V4Swap => {
                envelopes.extend(command_decode::v4_swap::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            OpcodeKind::ExecuteSubPlan => {
                let (sub_commands, sub_inputs) =
                    SubPlanInput::abi_decode_sequence(input, true).map_err(|e| {
                        AdapterError::Invalid(format!(
                            "EXECUTE_SUB_PLAN outer decode failed: {e}"
                        ))
                    })?;
                let sub_commands = sub_commands.to_vec();
                let sub_inputs: Vec<Vec<u8>> =
                    sub_inputs.into_iter().map(|b| b.to_vec()).collect();
                envelopes.extend(expand_commands(
                    ctx,
                    &sub_commands,
                    &sub_inputs,
                    validity.clone(),
                    depth + 1,
                    oc,
                )?);
            }
            OpcodeKind::Ignored => {}
            OpcodeKind::Unknown => {
                return Err(AdapterError::Invalid(format!(
                    "unsupported Universal Router command 0x{opcode:02x}"
                )));
            }
        }
    }

    Ok(envelopes)
}
