//! Universal Router command dispatch and shared command helpers.

use crate::command_decode::{
    v2_swap_exact_in, v2_swap_exact_out, v3_swap_exact_in, v3_swap_exact_out, v4_swap,
};
use crate::common::{currency_to_policy_address, shift_decimals, TokenLookup};
use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::{sol, SolType};
use policy_engine::prelude::*;
use serde_json::Value;

type SubPlanInput = sol! { (bytes, bytes[]) };

const FLAG_ALLOW_REVERT: u8 = 0x80;
const COMMAND_TYPE_MASK: u8 = 0x7f;

pub(crate) const V3_SWAP_EXACT_IN: u8 = 0x00;
pub(crate) const V3_SWAP_EXACT_OUT: u8 = 0x01;
const PERMIT2_TRANSFER_FROM: u8 = 0x02;
const PERMIT2_PERMIT_BATCH: u8 = 0x03;
const SWEEP: u8 = 0x04;
const TRANSFER: u8 = 0x05;
const PAY_PORTION: u8 = 0x06;
const PAY_PORTION_FULL_PRECISION: u8 = 0x07;
pub(crate) const V2_SWAP_EXACT_IN: u8 = 0x08;
pub(crate) const V2_SWAP_EXACT_OUT: u8 = 0x09;
const PERMIT2_PERMIT: u8 = 0x0a;
const WRAP_ETH: u8 = 0x0b;
const UNWRAP_WETH: u8 = 0x0c;
const PERMIT2_TRANSFER_FROM_BATCH: u8 = 0x0d;
const BALANCE_CHECK_ERC20: u8 = 0x0e;
pub(crate) const V4_SWAP: u8 = 0x10;
const V3_POSITION_MANAGER_PERMIT: u8 = 0x11;
const V3_POSITION_MANAGER_CALL: u8 = 0x12;
const V4_INITIALIZE_POOL: u8 = 0x13;
const V4_POSITION_MANAGER_CALL: u8 = 0x14;
const EXECUTE_SUB_PLAN: u8 = 0x21;

const MAX_DEPTH: usize = 4;
const MAX_COMMANDS: usize = 64;

const ACTION_MSG_SENDER: &str = "0x0000000000000000000000000000000000000001";
const ACTION_ADDRESS_THIS: &str = "0x0000000000000000000000000000000000000002";

#[derive(Debug, Clone)]
pub(crate) struct ActionMeta {
    pub(crate) command_index: usize,
    pub(crate) command_type: u8,
    pub(crate) allow_revert: bool,
    pub(crate) analysis_depth: &'static str,
    pub(crate) subplan_depth: usize,
    pub(crate) v4_action: Option<u8>,
    pub(crate) hook_data_present: bool,
}

pub(crate) struct RoutedAction {
    pub(crate) action: Action,
    pub(crate) meta: ActionMeta,
}

pub(crate) fn expand_commands(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    commands: &[u8],
    inputs: &[Vec<u8>],
    depth: usize,
) -> Result<Vec<RoutedAction>, AdapterError> {
    if depth > MAX_DEPTH {
        return Err(AdapterError::BadCalldata(format!(
            "Universal Router sub-plan depth exceeds max {MAX_DEPTH}"
        )));
    }
    if commands.len() != inputs.len() {
        return Err(AdapterError::BadCalldata(format!(
            "Universal Router length mismatch: {} commands, {} inputs",
            commands.len(),
            inputs.len()
        )));
    }
    if commands.len() > MAX_COMMANDS {
        return Err(AdapterError::BadCalldata(format!(
            "Universal Router command count {} exceeds max {MAX_COMMANDS}",
            commands.len()
        )));
    }

    let mut out = Vec::new();
    for (idx, raw_command) in commands.iter().copied().enumerate() {
        let allow_revert = raw_command & FLAG_ALLOW_REVERT != 0;
        let command = raw_command & COMMAND_TYPE_MASK;
        let meta = ActionMeta {
            command_index: idx,
            command_type: command,
            allow_revert,
            analysis_depth: "leaf",
            subplan_depth: depth,
            v4_action: None,
            hook_data_present: false,
        };
        let input = &inputs[idx];

        match command {
            V3_SWAP_EXACT_IN => out.push(v3_swap_exact_in::decode(tx, tokens, input, meta)?),
            V3_SWAP_EXACT_OUT => out.push(v3_swap_exact_out::decode(tx, tokens, input, meta)?),
            V2_SWAP_EXACT_IN => out.push(v2_swap_exact_in::decode(tx, tokens, input, meta)?),
            V2_SWAP_EXACT_OUT => out.push(v2_swap_exact_out::decode(tx, tokens, input, meta)?),
            V4_SWAP => out.extend(v4_swap::decode(tx, tokens, input, meta)?),
            EXECUTE_SUB_PLAN => {
                let (sub_commands, sub_inputs) = SubPlanInput::abi_decode_sequence(input, true)
                    .map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
                let sub_commands = sub_commands.to_vec();
                let sub_inputs = sub_inputs
                    .into_iter()
                    .map(|b| b.to_vec())
                    .collect::<Vec<_>>();
                out.extend(expand_commands(
                    tx,
                    tokens,
                    &sub_commands,
                    &sub_inputs,
                    depth + 1,
                )?);
            }
            PERMIT2_TRANSFER_FROM
            | PERMIT2_PERMIT_BATCH
            | SWEEP
            | TRANSFER
            | PAY_PORTION
            | PAY_PORTION_FULL_PRECISION
            | PERMIT2_PERMIT
            | WRAP_ETH
            | UNWRAP_WETH
            | PERMIT2_TRANSFER_FROM_BATCH
            | BALANCE_CHECK_ERC20
            | V3_POSITION_MANAGER_PERMIT
            | V3_POSITION_MANAGER_CALL
            | V4_INITIALIZE_POOL
            | V4_POSITION_MANAGER_CALL => {
                // Recognized non-swap commands. They are intentionally
                // ignored by the swap-policy leaf pass.
            }
            other => {
                return Err(AdapterError::BadCalldata(format!(
                    "unsupported Universal Router command 0x{other:02x}"
                )));
            }
        }
    }
    Ok(out)
}

pub(crate) fn add_meta(req: &mut PolicyRequest, meta: &ActionMeta) {
    let Some(obj) = req.context.as_object_mut() else {
        return;
    };
    obj.insert("router".into(), Value::from("universal-router"));
    obj.insert(
        "routerCommandIndex".into(),
        Value::from(meta.command_index as i64),
    );
    obj.insert(
        "routerCommand".into(),
        Value::from(format!("0x{:02x}", meta.command_type)),
    );
    obj.insert("allowRevert".into(), Value::from(meta.allow_revert));
    obj.insert("analysisDepth".into(), Value::from(meta.analysis_depth));
    obj.insert(
        "subplanDepth".into(),
        Value::from(meta.subplan_depth as i64),
    );
    obj.insert(
        "hookDataPresent".into(),
        Value::from(meta.hook_data_present),
    );
    if let Some(action) = meta.v4_action {
        obj.insert("v4Action".into(), Value::from(format!("0x{action:02x}")));
    }
}

pub(crate) fn token(tokens: &TokenLookup, chain_id: ChainId, address: AlloyAddress) -> Token {
    let addr = currency_to_policy_address(address);
    tokens.get(chain_id, &addr)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn swap_action(
    tx: &TransactionRequest,
    protocol_id: &str,
    input_token: Token,
    output_token: Token,
    input_amount: U256,
    min_output_amount: U256,
    recipient: Address,
    fee_bips: Option<u32>,
) -> Action {
    let human_input = shift_decimals(&input_amount.to_string(), input_token.decimals);
    let human_min_out = shift_decimals(&min_output_amount.to_string(), output_token.decimals);
    Action::Swap(SwapAction {
        protocol_id: protocol_id.into(),
        actor: tx.from.clone(),
        target: tx.to.clone(),
        value_wei: tx.value_wei.clone(),
        input_token: input_token.clone(),
        output_token: output_token.clone(),
        input_amount: AmountSpec {
            token: input_token,
            raw: input_amount.to_string(),
            human: Some(human_input),
            usd: None,
        },
        min_output_amount: Some(AmountSpec {
            token: output_token,
            raw: min_output_amount.to_string(),
            human: Some(human_min_out),
            usd: None,
        }),
        recipient,
        deadline: None,
        fee_bips,
    })
}

pub(crate) fn decode_v3_path(path: &[u8]) -> Result<(Vec<AlloyAddress>, Vec<u32>), AdapterError> {
    if path.len() < 43 || !(path.len() - 20).is_multiple_of(23) {
        return Err(AdapterError::BadCalldata(format!(
            "invalid v3 path length: {}",
            path.len()
        )));
    }
    let hops = (path.len() - 20) / 23;
    let mut tokens = Vec::with_capacity(hops + 1);
    let mut fees = Vec::with_capacity(hops);
    let mut cursor = 0;
    for _ in 0..hops {
        tokens.push(AlloyAddress::from_slice(&path[cursor..cursor + 20]));
        cursor += 20;
        let fee = ((path[cursor] as u32) << 16)
            | ((path[cursor + 1] as u32) << 8)
            | (path[cursor + 2] as u32);
        fees.push(fee);
        cursor += 3;
    }
    tokens.push(AlloyAddress::from_slice(&path[cursor..cursor + 20]));
    Ok((tokens, fees))
}

pub(crate) fn fee_bips_avg(fees: &[u32]) -> Option<u32> {
    if fees.is_empty() {
        None
    } else {
        Some(fees.iter().sum::<u32>() / (fees.len() as u32) / 100)
    }
}

pub(crate) fn map_recipient(tx: &TransactionRequest, recipient: AlloyAddress) -> Address {
    let recipient = Address::from_alloy(recipient);
    if recipient.as_str() == ACTION_MSG_SENDER {
        tx.from.clone()
    } else if recipient.as_str() == ACTION_ADDRESS_THIS {
        tx.to.clone()
    } else {
        recipient
    }
}
