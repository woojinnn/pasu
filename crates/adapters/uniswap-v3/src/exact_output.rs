//! Uniswap V3 SwapRouter `exactOutput` — multi-hop, exact-out. The path is
//! reversed in V3 semantics for exact-out (last token is `tokenIn`, first is
//! `tokenOut`); see Uniswap V3 docs §SwapRouter.

use crate::common::{
    decode_v3_path, shift_decimals, DecodeError, TokenLookup, SWAP_ROUTER_MAINNET,
};
use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;
use std::str::FromStr;

sol! {
    #[derive(Debug)]
    struct SolExactOutputParams {
        bytes   path;
        address recipient;
        uint256 deadline;
        uint256 amountOut;
        uint256 amountInMaximum;
    }

    function exactOutput(SolExactOutputParams params) external payable returns (uint256 amountIn);
}

pub const SELECTOR: [u8; 4] = exactOutputCall::SELECTOR;

#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub path: Vec<u8>,
    pub recipient: AlloyAddress,
    pub deadline: U256,
    pub amount_out: U256,
    pub amount_in_maximum: U256,
}

pub fn encode(p: &Params) -> Vec<u8> {
    exactOutputCall {
        params: SolExactOutputParams {
            path: p.path.clone().into(),
            recipient: p.recipient,
            deadline: p.deadline,
            amountOut: p.amount_out,
            amountInMaximum: p.amount_in_maximum,
        },
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
    if selector != SELECTOR {
        return Err(DecodeError::BadSelector {
            got: hex::encode(selector),
            want: hex::encode(SELECTOR),
        });
    }
    let call = exactOutputCall::abi_decode(calldata, true)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
    Ok(Params {
        path: call.params.path.to_vec(),
        recipient: call.params.recipient,
        deadline: call.params.deadline,
        amount_out: call.params.amountOut,
        amount_in_maximum: call.params.amountInMaximum,
    })
}

pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    tokens: TokenLookup,
}

impl Adapter_ {
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, Address::new(SWAP_ROUTER_MAINNET).unwrap())],
            tokens: TokenLookup::with_mainnet_defaults(),
        }
    }
}

impl Default for Adapter_ {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for Adapter_ {
    fn id(&self) -> AdapterId {
        AdapterId::new("uniswap-v3/exactOutput@0.1.0")
    }

    fn match_keys(&self) -> Vec<MatchKey> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| MatchKey::exact(*chain, target.clone(), SELECTOR))
            .collect()
    }

    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let p = decode(&tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        let (alloy_tokens, fees) =
            decode_v3_path(&p.path).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;

        // exactOutput's path is reversed: first element is tokenOut, last is tokenIn.
        let token_out_addr = Address::from_alloy(*alloy_tokens.first().unwrap());
        let token_in_addr = Address::from_alloy(*alloy_tokens.last().unwrap());
        let recipient_addr = Address::from_alloy(p.recipient);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = self.tokens.get(tx.chain_id, &token_out_addr);

        let fee_bips_avg = if fees.is_empty() {
            None
        } else {
            let sum: u32 = fees.iter().sum();
            Some(sum / (fees.len() as u32) / 100)
        };

        let human_in_max = shift_decimals(&p.amount_in_maximum.to_string(), input_token.decimals);
        let human_out = shift_decimals(&p.amount_out.to_string(), output_token.decimals);

        Ok(Action::Swap(SwapAction {
            protocol_id: "uniswap-v3".into(),
            actor: tx.from.clone(),
            target: tx.to.clone(),
            value_wei: tx.value_wei.clone(),
            input_token: input_token.clone(),
            output_token: output_token.clone(),
            input_amount: AmountSpec {
                token: input_token,
                raw: p.amount_in_maximum.to_string(),
                human: Some(human_in_max),
                usd: None,
            },
            min_output_amount: Some(AmountSpec {
                token: output_token,
                raw: p.amount_out.to_string(),
                human: Some(human_out),
                usd: None,
            }),
            recipient: recipient_addr,
            deadline: u64::from_str(&p.deadline.to_string()).ok(),
            fee_bips: fee_bips_avg,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_path(token_a: &str, fee: u32, token_b: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(43);
        out.extend_from_slice(AlloyAddress::from_str(token_a).unwrap().as_slice());
        out.extend_from_slice(&fee.to_be_bytes()[1..4]);
        out.extend_from_slice(AlloyAddress::from_str(token_b).unwrap().as_slice());
        out
    }

    fn sample_params() -> Params {
        Params {
            // For exactOutput, path is [tokenOut][fee][tokenIn].
            path: build_path(
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", // WETH (out)
                3000,
                "0xdAC17F958D2ee523a2206206994597C13D831ec7", // USDT (in)
            ),
            recipient: AlloyAddress::from_str("0x1111111111111111111111111111111111111111")
                .unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_out: U256::from(1_000_000_000_000_000_000u64),
            amount_in_maximum: U256::from(4_000_000_000_u64),
        }
    }

    #[test]
    fn round_trip() {
        let p = sample_params();
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }

    #[test]
    fn selector_pin() {
        assert_eq!(SELECTOR, [0xf2, 0x8c, 0x04, 0x98]);
    }

    #[test]
    fn build_handles_reversed_path_correctly() {
        let adapter = Adapter_::new();
        let p = sample_params();
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data: encode(&p),
            gas: None,
            nonce: None,
        };
        match adapter.build(&tx).unwrap() {
            Action::Swap(s) => {
                // Path was [WETH, fee, USDT] but exact-out reads it reversed:
                // input = USDT (last), output = WETH (first).
                assert_eq!(s.input_token.symbol, "USDT");
                assert_eq!(s.output_token.symbol, "WETH");
            }
            _ => panic!("expected swap"),
        }
    }
}
