//! Universal Router command dispatch and shared command helpers.

use crate::command_decode::{
    decode_v2_swap_exact_in, decode_v2_swap_exact_out, decode_v3_swap_exact_in,
    decode_v3_swap_exact_out, decode_v4_swap,
};
use crate::common::{currency_to_policy_address, TokenLookup};
use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::{sol, SolType};
use policy_engine::prelude::*;

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

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActionMeta {
    pub(crate) allow_revert: bool,
    command_label: &'static str,
    action_label: Option<&'static str>,
}

impl ActionMeta {
    const fn new(command: u8, allow_revert: bool) -> Self {
        Self {
            allow_revert,
            command_label: command_label(command),
            action_label: None,
        }
    }

    pub(crate) const fn with_action_label(mut self, action_label: &'static str) -> Self {
        self.action_label = Some(action_label);
        self
    }

    fn function_label(&self) -> &'static str {
        self.action_label.unwrap_or(self.command_label)
    }

    pub(crate) fn trace_label(&self) -> String {
        self.action_label.map_or_else(
            || {
                format!(
                    "command={} allowRevert={}",
                    self.command_label, self.allow_revert
                )
            },
            |action_label| {
                format!(
                    "command={} action={} allowRevert={}",
                    self.command_label, action_label, self.allow_revert
                )
            },
        )
    }
}

#[derive(Debug)]
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
        let meta = ActionMeta::new(command, allow_revert);
        let input = &inputs[idx];

        match command {
            V3_SWAP_EXACT_IN => out.push(decode_v3_swap_exact_in(tx, tokens, input, meta)?),
            V3_SWAP_EXACT_OUT => out.push(decode_v3_swap_exact_out(tx, tokens, input, meta)?),
            V2_SWAP_EXACT_IN => out.push(decode_v2_swap_exact_in(tx, tokens, input, meta)?),
            V2_SWAP_EXACT_OUT => out.push(decode_v2_swap_exact_out(tx, tokens, input, meta)?),
            V4_SWAP => out.extend(decode_v4_swap(tx, tokens, input, &meta)?),
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
                // ignored by the dex aggregation pass.
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

pub(crate) fn token(tokens: &TokenLookup, chain_id: ChainId, address: AlloyAddress) -> Token {
    let addr = currency_to_policy_address(address);
    tokens.get(chain_id, &addr)
}

pub(crate) fn path_endpoints(
    path: &[AlloyAddress],
    label: &str,
) -> Result<(AlloyAddress, AlloyAddress), AdapterError> {
    if path.len() < 2 {
        return Err(AdapterError::BadCalldata(format!(
            "{label} path must contain at least 2 tokens"
        )));
    }
    Ok((path[0], path[path.len() - 1]))
}

const fn command_label(command: u8) -> &'static str {
    match command {
        V3_SWAP_EXACT_IN => "V3_SWAP_EXACT_IN",
        V3_SWAP_EXACT_OUT => "V3_SWAP_EXACT_OUT",
        V2_SWAP_EXACT_IN => "V2_SWAP_EXACT_IN",
        V2_SWAP_EXACT_OUT => "V2_SWAP_EXACT_OUT",
        V4_SWAP => "V4_SWAP",
        EXECUTE_SUB_PLAN => "EXECUTE_SUB_PLAN",
        _ => "UNKNOWN_COMMAND",
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn swap_action(
    tx: &TransactionRequest,
    protocol_id: &str,
    input_token: Token,
    output_token: Token,
    input_amount: U256,
    min_output_amount: U256,
    recipient: &Address,
    fee_bips: Option<u32>,
    meta: &ActionMeta,
) -> Action {
    Action::Dex(DexAction {
        actor: tx.from.clone(),
        target: tx.to.clone(),
        value_wei: tx.value_wei.clone(),
        facts: DexFacts {
            protocol_ids: vec![protocol_id.into()],
            input_tokens: vec![input_token.clone()],
            output_tokens: vec![output_token.clone()],
            max_fee_bps: fee_bips,
            has_zero_min_output: min_output_amount == U256::ZERO,
            has_external_recipient: recipient != &tx.from,
            ..DexFacts::default()
        },
        oracle_requirements: vec![
            OracleRequirement {
                kind: OracleRequirementKind::Input,
                token: input_token,
                raw_amount: input_amount.to_string(),
            },
            OracleRequirement {
                kind: OracleRequirementKind::MinOutput,
                token: output_token,
                raw_amount: min_output_amount.to_string(),
            },
        ],
        trace: DexTrace {
            steps: vec![format!(
                "protocol={} function={} allowRevert={}",
                protocol_id,
                meta.function_label(),
                meta.allow_revert
            )],
        },
    })
}

pub(crate) fn merge_dex_actions(
    tx: &TransactionRequest,
    routed_actions: Vec<RoutedAction>,
) -> Result<DexAction, AdapterError> {
    let mut facts = DexFacts::default();
    let mut oracle_requirements = Vec::new();
    let mut trace_steps = Vec::new();

    for routed in routed_actions {
        let RoutedAction { action, meta } = routed;
        let dex = match action {
            Action::Dex(dex) => dex,
            other => {
                return Err(AdapterError::BadCalldata(format!(
                    "Universal Router routed non-Dex action: {}",
                    other.kind()
                )));
            }
        };

        for protocol_id in dex.facts.protocol_ids {
            push_unique_string(&mut facts.protocol_ids, protocol_id);
        }
        for token in dex.facts.input_tokens {
            push_unique_token(&mut facts.input_tokens, token);
        }
        for token in dex.facts.output_tokens {
            push_unique_token(&mut facts.output_tokens, token);
        }

        if let Some(fee_bps) = dex.facts.max_fee_bps {
            facts.max_fee_bps = Some(facts.max_fee_bps.map_or(fee_bps, |max| max.max(fee_bps)));
        }
        facts.has_zero_min_output |= dex.facts.has_zero_min_output;
        facts.has_external_recipient |= dex.facts.has_external_recipient;
        oracle_requirements.extend(dex.oracle_requirements);

        let route_label = meta.trace_label();
        if dex.trace.steps.is_empty() {
            trace_steps.push(route_label);
        } else {
            trace_steps.extend(
                dex.trace
                    .steps
                    .into_iter()
                    .map(|step| format!("{route_label}: {step}")),
            );
        }
    }

    Ok(DexAction {
        actor: tx.from.clone(),
        target: tx.to.clone(),
        value_wei: tx.value_wei.clone(),
        facts,
        oracle_requirements,
        trace: DexTrace { steps: trace_steps },
    })
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn push_unique_token(tokens: &mut Vec<Token>, token: Token) {
    if !tokens.iter().any(|existing| existing.key() == token.key()) {
        tokens.push(token);
    }
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
        let fee = (u32::from(path[cursor]) << 16)
            | (u32::from(path[cursor + 1]) << 8)
            | u32::from(path[cursor + 2]);
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
        let len = u32::try_from(fees.len()).ok()?;
        Some(fees.iter().sum::<u32>() / len / 100)
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
