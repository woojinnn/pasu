//! Universal Router command-stream dispatcher.
//!
//! Walks the `(commands, inputs)` pair produced by [`super::execute::decode_outer_call`]
//! and emits an `ActionEnvelope` per recognized opcode. Per-opcode decoding lives
//! in [`super::command_decode`].

use policy_engine::action::{ActionEnvelope, Validity};

use crate::{AdapterError, CallContext};

use super::command_decode;

const V3_SWAP_EXACT_IN: u8 = 0x00;
const V3_SWAP_EXACT_OUT: u8 = 0x01;
const PERMIT2_TRANSFER_FROM: u8 = 0x02;
const PERMIT2_PERMIT_BATCH: u8 = 0x03;
const V2_SWAP_EXACT_IN: u8 = 0x08;
const V2_SWAP_EXACT_OUT: u8 = 0x09;
const PERMIT2_PERMIT: u8 = 0x0a;
const WRAP_ETH: u8 = 0x0b;
const UNWRAP_WETH: u8 = 0x0c;
const PERMIT2_TRANSFER_FROM_BATCH: u8 = 0x0d;
const V4_SWAP_OPCODE: u8 = 0x10;
const COMMAND_TYPE_MASK: u8 = 0x7f;

/// Walk a UR command stream, dispatching each opcode to its decoder and
/// collecting the resulting envelopes.
pub(super) fn expand_commands(
    ctx: &CallContext<'_>,
    commands: &[u8],
    inputs: &[Vec<u8>],
    validity: Option<Validity>,
) -> Result<Vec<ActionEnvelope>, AdapterError> {
    let mut envelopes = Vec::new();

    for (index, raw_opcode) in commands.iter().copied().enumerate() {
        let Some(input) = inputs.get(index) else {
            return Err(AdapterError::Invalid(format!(
                "Universal Router missing input for command index {index}"
            )));
        };
        let opcode = raw_opcode & COMMAND_TYPE_MASK;
        match opcode {
            V3_SWAP_EXACT_IN => {
                envelopes.push(command_decode::v3_swap_exact_in::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            V3_SWAP_EXACT_OUT => {
                envelopes.push(command_decode::v3_swap_exact_out::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            V2_SWAP_EXACT_IN => {
                envelopes.push(command_decode::v2_swap_exact_in::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            V2_SWAP_EXACT_OUT => {
                envelopes.push(command_decode::v2_swap_exact_out::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            WRAP_ETH => {
                envelopes.push(command_decode::wrap_eth::decode(ctx, input)?);
            }
            UNWRAP_WETH => {
                envelopes.push(command_decode::unwrap_weth::decode(ctx, input)?);
            }
            V4_SWAP_OPCODE => {
                envelopes.extend(command_decode::v4_swap::decode(
                    ctx,
                    input,
                    validity.clone(),
                )?);
            }
            // Permit2 family — recognised explicitly so we don't treat them as
            // "unknown opcode" surprises. The permit semantics are gated on
            // the *sign* side by `sign_resolver::adapters::permit2`, which
            // evaluates the typed-data signature the wallet showed the user
            // *before* the swap calldata was even built. Inside UR these
            // commands just replay the same permit (or `transferFrom`) the
            // user already authorised, so we emit no extra envelope here.
            PERMIT2_PERMIT
            | PERMIT2_PERMIT_BATCH
            | PERMIT2_TRANSFER_FROM
            | PERMIT2_TRANSFER_FROM_BATCH => {}
            _ => {}
        }
    }

    Ok(envelopes)
}
