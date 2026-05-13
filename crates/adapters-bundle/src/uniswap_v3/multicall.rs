//! Uniswap V3 router `multicall` expansion.
//!
//! This adapter treats multicall as structural: each supported child calldata
//! is decoded into a leaf DEX action and merged into one aggregate DEX action.

#[cfg(test)]
use super::common::SWAP_ROUTER_MAINNET;
use crate::uniswap_v3::{
    common::{merge_dex_actions, static_adapter_id, swap_router_address, DecodeError},
    exact_input, exact_input_single, exact_output, exact_output_single,
};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

sol! {
    function multicall(bytes[] data) external payable returns (bytes[] results);
    function multicall(uint256 deadline, bytes[] data) external payable returns (bytes[] results);
}

/// Selector for `multicall(bytes[])`.
pub const SELECTOR_NO_DEADLINE: [u8; 4] = multicall_0Call::SELECTOR;
/// Selector for `multicall(uint256,bytes[])`.
pub const SELECTOR_DEADLINE: [u8; 4] = multicall_1Call::SELECTOR;

const MAX_DEPTH: usize = 4;
const MAX_CHILDREN: usize = 32;

/// Decoded multicall parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Optional multicall deadline.
    pub deadline: Option<alloy_primitives::U256>,
    /// Child calldata payloads.
    pub data: Vec<Vec<u8>>,
}

/// ABI-encode `multicall(bytes[])` calldata.
pub fn encode_no_deadline(data: Vec<Vec<u8>>) -> Vec<u8> {
    multicall_0Call {
        data: data.into_iter().map(Into::into).collect(),
    }
    .abi_encode()
}

/// ABI-encode `multicall(uint256,bytes[])` calldata.
pub fn encode_deadline(deadline: alloy_primitives::U256, data: Vec<Vec<u8>>) -> Vec<u8> {
    multicall_1Call {
        deadline,
        data: data.into_iter().map(Into::into).collect(),
    }
    .abi_encode()
}

/// Decode either supported multicall overload.
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

/// `TransactionActionAdapter` for `SwapRouter` multicall expansion.
#[derive(Debug)]
pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    exact_input_single: exact_input_single::Adapter_,
    exact_input: exact_input::Adapter_,
    exact_output_single: exact_output_single::Adapter_,
    exact_output: exact_output::Adapter_,
}

impl Adapter_ {
    /// Construct an adapter with mainnet `SwapRouter` and child adapters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, swap_router_address())],
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
    ) -> Result<Vec<LegacyAction>, ActionAdapterError> {
        if depth > MAX_DEPTH {
            return Err(ActionAdapterError::BadCalldata(format!(
                "multicall nesting exceeds max depth {MAX_DEPTH}"
            )));
        }
        if calls.len() > MAX_CHILDREN {
            return Err(ActionAdapterError::BadCalldata(format!(
                "multicall child count {} exceeds max {MAX_CHILDREN}",
                calls.len()
            )));
        }

        let mut out = Vec::new();
        for child_data in calls {
            let Some(selector) = selector(&child_data) else {
                return Err(ActionAdapterError::BadCalldata(
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
                let nested = decode(&child_tx.data)
                    .map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
                out.push(self.build_from_calls(&child_tx, nested.data, depth + 1)?);
            } else if selector == exact_input_single::SELECTOR {
                out.push(self.exact_input_single.build_action(&child_tx)?);
            } else if selector == exact_input::SELECTOR {
                out.push(self.exact_input.build_action(&child_tx)?);
            } else if selector == exact_output_single::SELECTOR {
                out.push(self.exact_output_single.build_action(&child_tx)?);
            } else if selector == exact_output::SELECTOR {
                out.push(self.exact_output.build_action(&child_tx)?);
            } else {
                return Err(ActionAdapterError::BadCalldata(format!(
                    "unsupported multicall child selector 0x{}",
                    hex::encode(selector)
                )));
            }
        }
        Ok(out)
    }

    fn build_from_calls(
        &self,
        tx: &TransactionRequest,
        calls: Vec<Vec<u8>>,
        depth: usize,
    ) -> Result<LegacyAction, ActionAdapterError> {
        let actions = self.expand_calls(tx, calls, depth)?;
        merge_dex_actions(tx, actions, "multicall")
    }
}

impl Default for Adapter_ {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionActionAdapter for Adapter_ {
    fn id(&self) -> ActionAdapterId {
        static_adapter_id("uniswap-v3/multicall@0.1.0")
    }

    fn match_keys(&self) -> Vec<TransactionMatchKey> {
        self.chain_targets
            .iter()
            .flat_map(|(chain, target)| {
                [
                    TransactionMatchKey::exact(*chain, target.clone(), SELECTOR_NO_DEADLINE),
                    TransactionMatchKey::exact(*chain, target.clone(), SELECTOR_DEADLINE),
                ]
            })
            .collect()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<LegacyAction, ActionAdapterError> {
        let p = decode(&tx.data).map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
        self.build_from_calls(tx, p.data, 0)
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
    use crate::uniswap_v3::exact_input_single;
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
    fn build_merges_supported_children_into_dex_action() {
        let adapter = Adapter_::new();
        let action = adapter
            .build_action(&tx(encode_deadline(
                U256::from(1u64),
                vec![swap(50_000_000), swap(200_000_000)],
            )))
            .unwrap();
        match action {
            LegacyAction::Dex(d) => {
                assert_eq!(d.facts.protocol_ids, vec!["uniswap-v3"]);
                assert_eq!(d.facts.input_tokens.len(), 1);
                assert_eq!(d.facts.input_tokens[0].symbol, "USDT");
                assert_eq!(d.facts.output_tokens.len(), 1);
                assert_eq!(d.facts.output_tokens[0].symbol, "WETH");
                assert_eq!(d.facts.max_fee_bps, Some(30));
                assert_eq!(d.oracle_requirements.len(), 4);
                assert_eq!(d.oracle_requirements[0].raw_amount, "50000000");
                assert_eq!(d.oracle_requirements[2].raw_amount, "200000000");
                assert_eq!(
                    d.trace.steps,
                    vec!["multicall", "exactInputSingle", "exactInputSingle"]
                );
            }
            other => panic!("expected dex, got {other:?}"),
        }
    }
}
