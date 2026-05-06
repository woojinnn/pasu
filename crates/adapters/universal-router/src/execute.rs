//! Universal Router `execute(bytes,bytes[])`.

use crate::commands::{expand_commands, merge_dex_actions, RoutedAction};
#[cfg(test)]
use crate::common::UNIVERSAL_ROUTER_MAINNET;
use crate::common::{router_address, TokenLookup};
use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

sol! {
    function execute(bytes commands, bytes[] inputs) external payable;
}

/// Selector for `execute(bytes,bytes[])`.
pub const SELECTOR_EXECUTE: [u8; 4] = executeCall::SELECTOR;

/// Decoded Universal Router execute parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Command bytes.
    pub commands: Vec<u8>,
    /// ABI-encoded input payloads, one per command.
    pub inputs: Vec<Vec<u8>>,
    /// Optional deadline from the overloaded execute form.
    pub deadline: Option<U256>,
}

/// Errors returned by Universal Router execute decoders.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    /// Calldata is shorter than the minimum required length.
    #[error("calldata too short: need at least {need} bytes, got {got}")]
    TooShort {
        /// Required byte length.
        need: usize,
        /// Actual byte length.
        got: usize,
    },
    /// The four-byte selector does not match a supported execute overload.
    #[error("unexpected selector: got 0x{got}, expected 0x{want}")]
    BadSelector {
        /// Observed selector hex.
        got: String,
        /// Expected selector hex.
        want: String,
    },
    /// ABI decoding failed.
    #[error("ABI decode failed: {0}")]
    AbiDecode(String),
    /// Decoded command or action is not supported.
    #[error("{0}")]
    Unsupported(String),
}

/// ABI-encode `execute(bytes,bytes[])` calldata.
#[must_use]
pub fn encode_execute(commands: Vec<u8>, inputs: Vec<Vec<u8>>) -> Vec<u8> {
    executeCall {
        commands: commands.into(),
        inputs: inputs.into_iter().map(Into::into).collect(),
    }
    .abi_encode()
}

/// Decode either supported Universal Router execute overload.
///
/// # Errors
///
/// Returns an error when calldata is too short, has an unsupported selector, or
/// fails ABI decoding.
pub fn decode(calldata: &[u8]) -> Result<Params, DecodeError> {
    if calldata.len() < 4 {
        return Err(DecodeError::TooShort {
            need: 4,
            got: calldata.len(),
        });
    }

    let selector = [calldata[0], calldata[1], calldata[2], calldata[3]];
    if selector == SELECTOR_EXECUTE {
        let call = executeCall::abi_decode(calldata, true)
            .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
        return Ok(Params {
            commands: call.commands.to_vec(),
            inputs: call.inputs.into_iter().map(|b| b.to_vec()).collect(),
            deadline: None,
        });
    }
    if selector == crate::execute_deadline::SELECTOR_EXECUTE_DEADLINE {
        return crate::execute_deadline::decode(calldata);
    }

    Err(DecodeError::BadSelector {
        got: hex::encode(selector),
        want: format!(
            "{} or {}",
            hex::encode(SELECTOR_EXECUTE),
            hex::encode(crate::execute_deadline::SELECTOR_EXECUTE_DEADLINE)
        ),
    })
}

/// Adapter for Universal Router execute calls.
#[derive(Debug)]
pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    tokens: TokenLookup,
}

impl Adapter_ {
    /// Construct an adapter with mainnet Universal Router and default tokens.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, router_address())],
            tokens: TokenLookup::with_mainnet_defaults(),
        }
    }

    fn decode_routed_actions(
        &self,
        tx: &TransactionRequest,
    ) -> Result<Vec<RoutedAction>, AdapterError> {
        let params = decode(&tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        expand_commands(tx, &self.tokens, &params.commands, &params.inputs, 0)
    }
}

impl Default for Adapter_ {
    fn default() -> Self {
        Self::new()
    }
}

impl TypedAdapter for Adapter_ {
    const ADAPTER_ID: &'static str = "uniswap/universal-router@0.1.0";
    const PROTOCOL_ID: &'static str = "uniswap";
    const KIND: AdapterKind = AdapterKind::CompositeRouter;
    const FUNCTIONS: &'static [SolidityFunctionSpec] = &[
        SolidityFunctionSpec::new("execute", "execute(bytes,bytes[])", SELECTOR_EXECUTE),
        SolidityFunctionSpec::new(
            "execute",
            "execute(bytes,bytes[],uint256)",
            crate::execute_deadline::SELECTOR_EXECUTE_DEADLINE,
        ),
    ];
    const EMITTED_ACTIONS: &'static [ActionKind] = &[ActionKind::Dex];

    fn contract_targets(&self) -> Vec<ContractTarget> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| ContractTarget::new(*chain, target.clone()))
            .collect()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let routed_actions = self.decode_routed_actions(tx)?;
        Ok(Action::Dex(merge_dex_actions(tx, routed_actions)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{V2_SWAP_EXACT_IN, V3_SWAP_EXACT_IN, V4_SWAP};
    use crate::v4_actions::{V4_SETTLE_ALL, V4_SWAP_EXACT_IN_SINGLE, V4_TAKE_ALL};
    use alloy_primitives::{
        aliases::{I24, U24},
        Address as AlloyAddress,
    };
    use alloy_sol_types::SolValue;
    use std::str::FromStr;

    const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
    const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
    const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

    fn build_v3_path(token_a: &str, fee: u32, token_b: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(AlloyAddress::from_str(token_a).unwrap().as_slice());
        out.extend_from_slice(&fee.to_be_bytes()[1..4]);
        out.extend_from_slice(AlloyAddress::from_str(token_b).unwrap().as_slice());
        out
    }

    fn tx(data: Vec<u8>) -> TransactionRequest {
        TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(UNIVERSAL_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data,
            gas: None,
            nonce: None,
        }
    }

    #[test]
    fn selector_pins() {
        assert_eq!(SELECTOR_EXECUTE, [0x24, 0x85, 0x6b, 0xc3]);
        assert_eq!(
            crate::execute_deadline::SELECTOR_EXECUTE_DEADLINE,
            [0x35, 0x93, 0x56, 0x4c]
        );
    }

    #[test]
    fn round_trip_execute_with_deadline() {
        let calldata = crate::execute_deadline::encode_execute_deadline(
            vec![V3_SWAP_EXACT_IN],
            vec![vec![1, 2, 3]],
            U256::from(7),
        );
        let decoded = decode(&calldata).unwrap();
        assert_eq!(decoded.commands, vec![V3_SWAP_EXACT_IN]);
        assert_eq!(decoded.inputs, vec![vec![1, 2, 3]]);
        assert_eq!(decoded.deadline, Some(U256::from(7)));
    }

    #[test]
    fn v3_exact_in_command_emits_aggregate_dex_action() {
        let input = (
            AlloyAddress::from_str(RECIPIENT).unwrap(),
            U256::from(200_000_000u64),
            U256::ZERO,
            build_v3_path(USDT, 3000, WETH),
            true,
            Vec::<U256>::new(),
        )
            .abi_encode_sequence();
        let calldata = encode_execute(vec![V3_SWAP_EXACT_IN], vec![input]);
        let action = Adapter_::new().build_action(&tx(calldata)).unwrap();
        match &action {
            Action::Dex(dex) => {
                assert_eq!(dex.facts.protocol_ids, vec!["uniswap-v3"]);
                assert_eq!(symbols(&dex.facts.input_tokens), vec!["USDT"]);
                assert_eq!(symbols(&dex.facts.output_tokens), vec!["WETH"]);
                assert_eq!(dex.oracle_requirements.len(), 2);
                assert_eq!(
                    dex.oracle_requirements
                        .iter()
                        .find(|r| r.kind == OracleRequirementKind::Input)
                        .unwrap()
                        .raw_amount,
                    "200000000"
                );
                assert_eq!(dex.facts.max_fee_bps, Some(30));
                assert!(dex.facts.has_zero_min_output);
                assert!(dex.facts.has_external_recipient);
                assert!(dex
                    .trace
                    .steps
                    .iter()
                    .any(|step| step.contains("V3_SWAP_EXACT_IN")));
            }
            other @ Action::Other(_) => panic!("expected dex, got {other:?}"),
        }
    }

    #[test]
    fn v4_exact_in_single_command_emits_aggregate_dex_action() {
        let pool_key = crate::v4_actions::PoolKey {
            currency0: AlloyAddress::from_str(USDT).unwrap(),
            currency1: AlloyAddress::from_str(WETH).unwrap(),
            fee: U24::from(3000u32),
            tickSpacing: I24::try_from(60i32).unwrap(),
            hooks: AlloyAddress::ZERO,
        };
        let swap_params = crate::v4_actions::V4ExactInputSingleParams {
            poolKey: pool_key,
            zeroForOne: true,
            amountIn: 200_000_000u128,
            amountOutMinimum: 0u128,
            minHopPriceX36: U256::ZERO,
            hookData: Vec::<u8>::new().into(),
        }
        .abi_encode_sequence();
        let actions = vec![V4_SWAP_EXACT_IN_SINGLE, V4_SETTLE_ALL, V4_TAKE_ALL];
        let params = vec![
            swap_params,
            (
                AlloyAddress::from_str(USDT).unwrap(),
                U256::from(200_000_000u64),
            )
                .abi_encode_sequence(),
            (AlloyAddress::from_str(WETH).unwrap(), U256::ZERO).abi_encode_sequence(),
        ];
        let v4_input = (actions, params).abi_encode_sequence();
        let calldata = encode_execute(vec![V4_SWAP], vec![v4_input]);

        let action = Adapter_::new().build_action(&tx(calldata)).unwrap();
        match &action {
            Action::Dex(dex) => {
                assert_eq!(dex.facts.protocol_ids, vec!["uniswap-v4"]);
                assert_eq!(symbols(&dex.facts.input_tokens), vec!["USDT"]);
                assert_eq!(symbols(&dex.facts.output_tokens), vec!["WETH"]);
                assert_eq!(
                    dex.oracle_requirements
                        .iter()
                        .find(|r| r.kind == OracleRequirementKind::MinOutput)
                        .unwrap()
                        .raw_amount,
                    "0"
                );
                assert_eq!(dex.facts.max_fee_bps, Some(30));
                assert!(dex
                    .trace
                    .steps
                    .iter()
                    .any(|step| step.contains("V4_SWAP_EXACT_IN_SINGLE")));
            }
            other @ Action::Other(_) => panic!("expected dex, got {other:?}"),
        }
    }

    #[test]
    fn multiple_swap_commands_merge_into_one_dex_action() {
        let v3_input = (
            AlloyAddress::from_str(RECIPIENT).unwrap(),
            U256::from(200_000_000u64),
            U256::from(100_000_000u64),
            build_v3_path(USDT, 500, WETH),
            true,
            Vec::<U256>::new(),
        )
            .abi_encode_sequence();
        let v2_input = (
            AlloyAddress::from_str(RECIPIENT).unwrap(),
            U256::from(300_000_000u64),
            U256::ZERO,
            vec![
                AlloyAddress::from_str(USDT).unwrap(),
                AlloyAddress::from_str(WETH).unwrap(),
            ],
            true,
            Vec::<U256>::new(),
        )
            .abi_encode_sequence();
        let calldata = encode_execute(
            vec![V3_SWAP_EXACT_IN, V2_SWAP_EXACT_IN],
            vec![v3_input, v2_input],
        );

        let action = Adapter_::new().build_action(&tx(calldata)).unwrap();
        match &action {
            Action::Dex(dex) => {
                assert_eq!(dex.facts.protocol_ids, vec!["uniswap-v3", "uniswap-v2"]);
                assert_eq!(symbols(&dex.facts.input_tokens), vec!["USDT"]);
                assert_eq!(symbols(&dex.facts.output_tokens), vec!["WETH"]);
                assert_eq!(dex.facts.max_fee_bps, Some(30));
                assert!(dex.facts.has_zero_min_output);
                assert_eq!(
                    input_raw_amounts(&dex.oracle_requirements),
                    vec!["200000000", "300000000"]
                );
                assert!(dex
                    .trace
                    .steps
                    .iter()
                    .any(|step| step.contains("V3_SWAP_EXACT_IN")));
                assert!(dex
                    .trace
                    .steps
                    .iter()
                    .any(|step| step.contains("V2_SWAP_EXACT_IN")));
            }
            other @ Action::Other(_) => panic!("expected dex, got {other:?}"),
        }
    }

    fn input_raw_amounts(requirements: &[OracleRequirement]) -> Vec<&str> {
        requirements
            .iter()
            .filter(|requirement| requirement.kind == OracleRequirementKind::Input)
            .map(|requirement| requirement.raw_amount.as_str())
            .collect()
    }

    fn symbols(tokens: &[Token]) -> Vec<&str> {
        tokens.iter().map(|token| token.symbol.as_str()).collect()
    }
}
