//! Universal Router command `V4_SWAP`.

use crate::universal_router::commands::{ActionMeta, RoutedAction};
use super::super::common::TokenLookup;
use crate::universal_router::v4_actions::{
    decode_exact_input, decode_exact_input_single, decode_exact_output, decode_exact_output_single,
    MAX_V4_ACTIONS, V4_CLEAR_OR_TAKE, V4_CLOSE_CURRENCY, V4_SETTLE, V4_SETTLE_ALL, V4_SETTLE_PAIR,
    V4_SWAP_EXACT_IN, V4_SWAP_EXACT_IN_SINGLE, V4_SWAP_EXACT_OUT, V4_SWAP_EXACT_OUT_SINGLE,
    V4_SWEEP, V4_TAKE, V4_TAKE_ALL, V4_TAKE_PAIR, V4_TAKE_PORTION, V4_UNWRAP, V4_WRAP,
};
use alloy_sol_types::{sol, SolType};
use policy_engine::prelude::*;

type Input = sol! { (bytes, bytes[]) };

pub(super) fn decode(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    base_meta: &ActionMeta,
) -> Result<Vec<RoutedAction>, ActionAdapterError> {
    let (actions, params) = Input::abi_decode_sequence(input, true)
        .map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
    let actions = actions.to_vec();
    if actions.len() != params.len() {
        return Err(ActionAdapterError::BadCalldata(format!(
            "v4 action length mismatch: {} actions, {} params",
            actions.len(),
            params.len()
        )));
    }
    if actions.len() > MAX_V4_ACTIONS {
        return Err(ActionAdapterError::BadCalldata(format!(
            "v4 action count {} exceeds max {MAX_V4_ACTIONS}",
            actions.len()
        )));
    }

    let mut out = Vec::new();
    for (idx, action) in actions.iter().copied().enumerate() {
        let meta = base_meta.with_action_label(v4_action_label(action));
        let input = params[idx].to_vec();
        match action {
            V4_SWAP_EXACT_IN_SINGLE => {
                out.push(decode_exact_input_single(tx, tokens, &input, meta)?);
            }
            V4_SWAP_EXACT_IN => out.push(decode_exact_input(tx, tokens, &input, meta)?),
            V4_SWAP_EXACT_OUT_SINGLE => {
                out.push(decode_exact_output_single(tx, tokens, &input, meta)?);
            }
            V4_SWAP_EXACT_OUT => out.push(decode_exact_output(tx, tokens, &input, meta)?),
            V4_SETTLE | V4_SETTLE_ALL | V4_SETTLE_PAIR | V4_TAKE | V4_TAKE_ALL
            | V4_TAKE_PORTION | V4_TAKE_PAIR | V4_CLOSE_CURRENCY | V4_CLEAR_OR_TAKE | V4_SWEEP
            | V4_WRAP | V4_UNWRAP => {}
            other => {
                return Err(ActionAdapterError::BadCalldata(format!(
                    "unsupported v4 action 0x{other:02x}"
                )));
            }
        }
    }
    Ok(out)
}

const fn v4_action_label(action: u8) -> &'static str {
    match action {
        V4_SWAP_EXACT_IN_SINGLE => "V4_SWAP_EXACT_IN_SINGLE",
        V4_SWAP_EXACT_IN => "V4_SWAP_EXACT_IN",
        V4_SWAP_EXACT_OUT_SINGLE => "V4_SWAP_EXACT_OUT_SINGLE",
        V4_SWAP_EXACT_OUT => "V4_SWAP_EXACT_OUT",
        V4_SETTLE => "V4_SETTLE",
        V4_SETTLE_ALL => "V4_SETTLE_ALL",
        V4_SETTLE_PAIR => "V4_SETTLE_PAIR",
        V4_TAKE => "V4_TAKE",
        V4_TAKE_ALL => "V4_TAKE_ALL",
        V4_TAKE_PORTION => "V4_TAKE_PORTION",
        V4_TAKE_PAIR => "V4_TAKE_PAIR",
        V4_CLOSE_CURRENCY => "V4_CLOSE_CURRENCY",
        V4_CLEAR_OR_TAKE => "V4_CLEAR_OR_TAKE",
        V4_SWEEP => "V4_SWEEP",
        V4_WRAP => "V4_WRAP",
        V4_UNWRAP => "V4_UNWRAP",
        _ => "UNKNOWN_V4_ACTION",
    }
}
