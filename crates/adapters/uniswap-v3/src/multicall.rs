//! Uniswap V3 router `multicall` expansion.
//!
//! This adapter treats multicall as structural: each supported child calldata
//! is decoded into its own leaf `Action`, and the pipeline evaluates the
//! existing leaf policies against those actions.

use crate::{
    common::{DecodeError, SWAP_ROUTER_MAINNET},
    exact_input, exact_input_single, exact_output, exact_output_single,
};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

sol! {
    function multicall(bytes[] data) external payable returns (bytes[] results);
    function multicall(uint256 deadline, bytes[] data) external payable returns (bytes[] results);
}

pub const SELECTOR_NO_DEADLINE: [u8; 4] = multicall_0Call::SELECTOR;
pub const SELECTOR_DEADLINE: [u8; 4] = multicall_1Call::SELECTOR;

const MAX_DEPTH: usize = 4;
const MAX_CHILDREN: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub deadline: Option<alloy_primitives::U256>,
    pub data: Vec<Vec<u8>>,
}

pub fn encode_no_deadline(data: Vec<Vec<u8>>) -> Vec<u8> {
    multicall_0Call {
        data: data.into_iter().map(Into::into).collect(),
    }
    .abi_encode()
}

pub fn encode_deadline(deadline: alloy_primitives::U256, data: Vec<Vec<u8>>) -> Vec<u8> {
    multicall_1Call {
        deadline,
        data: data.into_iter().map(Into::into).collect(),
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

    let selector: [u8; 4] = [calldata[0], calldata[1], calldata[2], calldata[3]];
    if selector == SELECTOR_NO_DEADLINE {
        let call = multicall_0Call::abi_decode(calldata, true)
            .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
        return Ok(Params {
            deadline: None,
            data: call.data.into_iter().map(|b| b.to_vec()).collect(),
        });
    }
    if selector == SELECTOR_DEADLINE {
        let call = multicall_1Call::abi_decode(calldata, true)
            .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
        return Ok(Params {
            deadline: Some(call.deadline),
            data: call.data.into_iter().map(|b| b.to_vec()).collect(),
        });
    }

    Err(DecodeError::BadSelector {
        got: hex::encode(selector),
        want: format!(
            "{} or {}",
            hex::encode(SELECTOR_NO_DEADLINE),
            hex::encode(SELECTOR_DEADLINE)
        ),
    })
}

pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    exact_input_single: exact_input_single::Adapter_,
    exact_input: exact_input::Adapter_,
    exact_output_single: exact_output_single::Adapter_,
    exact_output: exact_output::Adapter_,
}

impl Adapter_ {
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, Address::new(SWAP_ROUTER_MAINNET).unwrap())],
            exact_input_single: exact_input_single::Adapter_::new(),
            exact_input: exact_input::Adapter_::new(),
            exact_output_single: exact_output_single::Adapter_::new(),
            exact_output: exact_output::Adapter_::new(),
        }
    }

    fn expand_calls(
        &self,
        tx: &TransactionRequest,
        calls: Vec<Vec<u8>>,
        depth: usize,
    ) -> Result<Vec<Action>, AdapterError> {
        if depth > MAX_DEPTH {
            return Err(AdapterError::BadCalldata(format!(
                "multicall nesting exceeds max depth {MAX_DEPTH}"
            )));
        }
        if calls.len() > MAX_CHILDREN {
            return Err(AdapterError::BadCalldata(format!(
                "multicall child count {} exceeds max {MAX_CHILDREN}",
                calls.len()
            )));
        }

        let mut out = Vec::new();
        for child_data in calls {
            let Some(selector) = selector(&child_data) else {
                return Err(AdapterError::BadCalldata(
                    "multicall child calldata too short".into(),
                ));
            };
            let child_tx = TransactionRequest {
                chain_id: tx.chain_id,
                from: tx.from.clone(),
                to: tx.to.clone(),
                value_wei: tx.value_wei.clone(),
                data: child_data,
                gas: tx.gas,
                nonce: tx.nonce,
            };

            if selector == SELECTOR_NO_DEADLINE || selector == SELECTOR_DEADLINE {
                let nested =
                    decode(&child_tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
                out.extend(self.expand_calls(&child_tx, nested.data, depth + 1)?);
            } else if selector == exact_input_single::SELECTOR {
                out.push(self.exact_input_single.build(&child_tx)?);
            } else if selector == exact_input::SELECTOR {
                out.push(self.exact_input.build(&child_tx)?);
            } else if selector == exact_output_single::SELECTOR {
                out.push(self.exact_output_single.build(&child_tx)?);
            } else if selector == exact_output::SELECTOR {
                out.push(self.exact_output.build(&child_tx)?);
            } else {
                return Err(AdapterError::BadCalldata(format!(
                    "unsupported multicall child selector 0x{}",
                    hex::encode(selector)
                )));
            }
        }
        Ok(out)
    }
}

impl Default for Adapter_ {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for Adapter_ {
    fn id(&self) -> AdapterId {
        AdapterId::new("uniswap-v3/multicall@0.1.0")
    }

    fn match_keys(&self) -> Vec<MatchKey> {
        self.chain_targets
            .iter()
            .flat_map(|(chain, target)| {
                [
                    MatchKey::exact(*chain, target.clone(), SELECTOR_NO_DEADLINE),
                    MatchKey::exact(*chain, target.clone(), SELECTOR_DEADLINE),
                ]
            })
            .collect()
    }

    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let actions = self.build_actions(tx)?;
        Ok(Action::Multi(MultiAction {
            actor: tx.from.clone(),
            target: tx.to.clone(),
            value_wei: tx.value_wei.clone(),
            children: actions,
        }))
    }

    fn build_actions(&self, tx: &TransactionRequest) -> Result<Vec<Action>, AdapterError> {
        let p = decode(&tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        self.expand_calls(tx, p.data, 0)
    }
}

fn selector(data: &[u8]) -> Option<[u8; 4]> {
    if data.len() < 4 {
        None
    } else {
        Some([data[0], data[1], data[2], data[3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exact_input_single;
    use alloy_primitives::{Address as AlloyAddress, U256};
    use std::str::FromStr;

    const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
    const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
    const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

    fn swap(amount_in: u64) -> Vec<u8> {
        exact_input_single::encode(&exact_input_single::Params {
            token_in: AlloyAddress::from_str(USDT).unwrap(),
            token_out: AlloyAddress::from_str(WETH).unwrap(),
            fee: 3000,
            recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_in: U256::from(amount_in),
            amount_out_minimum: U256::ZERO,
            sqrt_price_limit_x96: U256::ZERO,
        })
    }

    fn tx(data: Vec<u8>) -> TransactionRequest {
        TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data,
            gas: None,
            nonce: None,
        }
    }

    #[test]
    fn selector_pins() {
        assert_eq!(SELECTOR_NO_DEADLINE, [0xac, 0x96, 0x50, 0xd8]);
        assert_eq!(SELECTOR_DEADLINE, [0x5a, 0xe4, 0x01, 0xdc]);
    }

    #[test]
    fn round_trip_no_deadline() {
        let data = vec![swap(50_000_000), swap(200_000_000)];
        let decoded = decode(&encode_no_deadline(data.clone())).unwrap();
        assert_eq!(decoded.deadline, None);
        assert_eq!(decoded.data, data);
    }

    #[test]
    fn build_actions_expands_supported_children() {
        let adapter = Adapter_::new();
        let actions = adapter
            .build_actions(&tx(encode_deadline(
                U256::from(1u64),
                vec![swap(200_000_000)],
            )))
            .unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Swap(s) => assert_eq!(s.input_amount.raw, "200000000"),
            other => panic!("expected swap, got {other:?}"),
        }
    }
}
