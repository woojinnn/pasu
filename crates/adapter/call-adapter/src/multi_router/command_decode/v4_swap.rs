//! UR command 0x10 V4_SWAP —
//! `input = abi.encode(bytes actions, bytes[] params)` per V4Router. The
//! action byte string is iterated like UR's own command stream but against
//! `V4_ROUTER_TABLE` (which provides per-action JSON-ABI schemas for the inner
//! params bytes). Only the 4 swap actions emit `SwapAction` envelopes;
//! settle/take/delta-management actions are intentionally skipped today.

use abi_resolver::subdecode::opcode_stream::dispatch as dispatch_opcodes;
use abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE;
use alloy_sol_types::{sol, SolValue};
use policy_engine::action::{ActionEnvelope, Validity};

use crate::{AdapterError, CallContext};

use super::super::v4_actions::{exact_input, exact_input_single, exact_output, exact_output_single};

// Inner V4 action opcodes (dispatched against V4_ROUTER_TABLE).
const V4_ACTION_SWAP_EXACT_IN_SINGLE: u8 = 0x06;
const V4_ACTION_SWAP_EXACT_IN: u8 = 0x07;
const V4_ACTION_SWAP_EXACT_OUT_SINGLE: u8 = 0x08;
const V4_ACTION_SWAP_EXACT_OUT: u8 = 0x09;

sol! {
    #[allow(clippy::too_many_arguments)]
    struct V4SwapInput {
        bytes actions;
        bytes[] params;
    }
}

pub(in crate::multi_router) fn decode(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<Vec<ActionEnvelope>, AdapterError> {
    let parsed = V4SwapInput::abi_decode_sequence(input, true)
        .map_err(|e| AdapterError::Invalid(format!("V4_SWAP outer decode failed: {e}")))?;
    let actions = parsed.actions.to_vec();
    let params: Vec<Vec<u8>> = parsed.params.iter().map(|b| b.to_vec()).collect();
    let steps = dispatch_opcodes(&actions, &params, &V4_ROUTER_TABLE);

    let mut out = Vec::new();
    for step in &steps {
        let env = match step.opcode {
            V4_ACTION_SWAP_EXACT_IN_SINGLE => exact_input_single::decode(ctx, step, validity.clone())?,
            V4_ACTION_SWAP_EXACT_IN => exact_input::decode(ctx, step, validity.clone())?,
            V4_ACTION_SWAP_EXACT_OUT_SINGLE => exact_output_single::decode(ctx, step, validity.clone())?,
            V4_ACTION_SWAP_EXACT_OUT => exact_output::decode(ctx, step, validity.clone())?,
            _ => continue,
        };
        out.push(env);
    }
    Ok(out)
}
