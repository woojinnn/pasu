//! Universal Router `execute(bytes,bytes[])`.

use crate::commands::{add_meta, expand_commands, RoutedAction};
use crate::common::{TokenLookup, UNIVERSAL_ROUTER_MAINNET};
use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;
use policy_engine::{enrich_with_usd, request_from_action};

sol! {
    function execute(bytes commands, bytes[] inputs) external payable;
}

pub const SELECTOR_EXECUTE: [u8; 4] = executeCall::SELECTOR;

#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub commands: Vec<u8>,
    pub inputs: Vec<Vec<u8>>,
    pub deadline: Option<U256>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("calldata too short: need at least {need} bytes, got {got}")]
    TooShort { need: usize, got: usize },
    #[error("unexpected selector: got 0x{got}, expected 0x{want}")]
    BadSelector { got: String, want: String },
    #[error("ABI decode failed: {0}")]
    AbiDecode(String),
    #[error("{0}")]
    Unsupported(String),
}

pub fn encode_execute(commands: Vec<u8>, inputs: Vec<Vec<u8>>) -> Vec<u8> {
    executeCall {
        commands: commands.into(),
        inputs: inputs.into_iter().map(Into::into).collect(),
    }
    .abi_encode()
}

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

pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    tokens: TokenLookup,
}

impl Adapter_ {
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, Address::new(UNIVERSAL_ROUTER_MAINNET).unwrap())],
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
    const EMITTED_ACTIONS: &'static [ActionKind] = &[ActionKind::Swap, ActionKind::Multi];

    fn contract_targets(&self) -> Vec<ContractTarget> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| ContractTarget::new(*chain, target.clone()))
            .collect()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let actions = self.build_leaf_actions(tx)?;
        Ok(Action::Multi(MultiAction {
            actor: tx.from.clone(),
            target: tx.to.clone(),
            value_wei: tx.value_wei.clone(),
            children: actions,
        }))
    }

    fn build_leaf_actions(&self, tx: &TransactionRequest) -> Result<Vec<Action>, AdapterError> {
        Ok(self
            .decode_routed_actions(tx)?
            .into_iter()
            .map(|r| r.action)
            .collect())
    }

    fn lower_requests(
        &self,
        tx: &TransactionRequest,
        oracle: &dyn Oracle,
    ) -> Result<Vec<PolicyRequest>, AdapterError> {
        let mut routed = self.decode_routed_actions(tx)?;
        let mut requests = Vec::with_capacity(routed.len());
        for routed_action in &mut routed {
            enrich_with_usd(&mut routed_action.action, oracle);
            let mut req = request_from_action(&routed_action.action);
            add_meta(&mut req, &routed_action.meta);
            requests.push(req);
        }
        Ok(requests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{V3_SWAP_EXACT_IN, V4_SWAP};
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
    fn v3_exact_in_command_emits_swap_leaf() {
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
        let actions = Adapter_::new().build_actions(&tx(calldata)).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Swap(s) => {
                assert_eq!(s.protocol_id, "uniswap-v3");
                assert_eq!(s.input_token.symbol, "USDT");
                assert_eq!(s.output_token.symbol, "WETH");
                assert_eq!(s.input_amount.raw, "200000000");
            }
            other => panic!("expected swap, got {other:?}"),
        }
    }

    #[test]
    fn v4_exact_in_single_command_emits_swap_leaf() {
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

        let actions = Adapter_::new().build_actions(&tx(calldata)).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Swap(s) => {
                assert_eq!(s.protocol_id, "uniswap-v4");
                assert_eq!(s.input_token.symbol, "USDT");
                assert_eq!(s.output_token.symbol, "WETH");
                assert_eq!(s.fee_bips, Some(30));
            }
            other => panic!("expected swap, got {other:?}"),
        }
    }
}
