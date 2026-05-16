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
    /// SWEEP — drain router balance to recipient, ≥ amountMin. Surfaced as
    /// a TransferAction so the simulator sees the router→user settlement
    /// (otherwise [WRAP, SWAP(→Router), SWEEP] can't collapse cleanly).
    Sweep,
    /// TRANSFER — send exact amount of token from router to recipient.
    /// Same modelling as SWEEP with `Exact` instead of `AtLeast`.
    Transfer,
    /// Recognised but intentionally not surfaced as an envelope (Permit2 family,
    /// PAY_PORTION, position-manager calls, balance checks). Distinct from
    /// `Unknown` so unrecognised opcodes still produce an error.
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

    /// PancakeSwap (Infinity) Universal Router opcode table.
    /// Mirrors Uniswap on the 0x00–0x0e common range, omits 0x07 (Pancake
    /// dispatcher reverts), maps 0x10 to Pancake Infinity (different from
    /// Uniswap V4 — treated as Ignored pending an Infinity decoder), and
    /// adds 0x22/0x23 stable-swap (Ignored pending a stable-swap decoder).
    /// The mask is `0x3f` (top two bits reserved by Pancake).
    pub const PANCAKE_UR: Self = Self {
        mask: 0x3f,
        classify: classify_pancake_ur,
    };
}

fn classify_uniswap_ur(opcode: u8) -> OpcodeKind {
    match opcode {
        0x00 => OpcodeKind::V3SwapExactIn,
        0x01 => OpcodeKind::V3SwapExactOut,
        0x04 => OpcodeKind::Sweep,         // surfaced as TransferAction for the simulator
        0x05 => OpcodeKind::Transfer,      // ditto
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
        //   PAY_PORTION (0x06) / PAY_PORTION_FULL_PRECISION (0x07): take a
        //   ratio of the router balance — exact amount unknowable without
        //   simulating the swap output. Skipping keeps the ledger sound;
        //   the simulator falls back to fan-out when the fee is large
        //   enough to matter.
        //
        //   BALANCE_CHECK_ERC20 (0x0e): assertion-only, no asset move.
        //
        //   V3/V4 position manager (0x11–0x14): liquidity-position operations
        //   outside the current swap policy scope.
        0x02 | 0x03 | 0x06 | 0x07 | 0x0a | 0x0d | 0x0e | 0x11 | 0x12 | 0x13 | 0x14 => {
            OpcodeKind::Ignored
        }
        _ => OpcodeKind::Unknown,
    }
}

fn classify_pancake_ur(opcode: u8) -> OpcodeKind {
    // Pancake forks Uniswap UR; the 0x00..=0x0e range is identical. Differences:
    //   - 0x07 is a placeholder in Pancake (Uniswap uses it for
    //     PAY_PORTION_FULL_PRECISION); Pancake's dispatcher reverts on it,
    //     so we surface as Unknown rather than silently ignore.
    //   - 0x10 is INFI_SWAP (Pancake Infinity, V4-like) instead of Uniswap's
    //     V4_SWAP. The two share the outer `(actions, params[])` shape but
    //     dispatch inner actions against a different table (PANCAKE_INFI_TABLE).
    //     Until a dedicated Infinity decoder lands we classify it Ignored —
    //     wallet sees the Pancake UR call but the Infinity sub-actions don't
    //     surface (better to under-report than mis-report against Uniswap V4).
    //   - 0x22/0x23 are Pancake stable-swap opcodes; Ignored for the same
    //     reason (no stable-swap decoder yet).
    match opcode {
        0x00 => OpcodeKind::V3SwapExactIn,
        0x01 => OpcodeKind::V3SwapExactOut,
        0x04 => OpcodeKind::Sweep,
        0x05 => OpcodeKind::Transfer,
        0x08 => OpcodeKind::V2SwapExactIn,
        0x09 => OpcodeKind::V2SwapExactOut,
        0x0b => OpcodeKind::WrapEth,
        0x0c => OpcodeKind::UnwrapWeth,
        0x21 => OpcodeKind::ExecuteSubPlan,
        // Permit2 family + PAY_PORTION + balance check (same as Uniswap)
        0x02 | 0x03 | 0x06 | 0x0a | 0x0d | 0x0e
        // V3 position manager + Pancake Infinity slots (0x10..=0x16)
        | 0x10 | 0x11 | 0x12 | 0x13 | 0x14 | 0x15 | 0x16
        // Pancake stable-swap
        | 0x22 | 0x23 => OpcodeKind::Ignored,
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
            OpcodeKind::Sweep => {
                envelopes.push(command_decode::sweep::decode(ctx, input)?);
            }
            OpcodeKind::Transfer => {
                envelopes.push(command_decode::transfer::decode(ctx, input)?);
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
