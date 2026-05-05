//! Uniswap V2 Router02 `swapExactTokensForETH(uint256 amountIn,
//! uint256 amountOutMin, address[] path, address to, uint256 deadline)`.
//! Output is native ETH delivered to `to`.

use crate::common::{
    native_eth, shift_decimals, DecodeError, TokenLookup, UNISWAP_V2_ROUTER_MAINNET,
};
use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;
use std::str::FromStr;

sol! {
    function swapExactTokensForETH(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[] amounts);
}

pub const SELECTOR: [u8; 4] = swapExactTokensForETHCall::SELECTOR;

#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub amount_in: U256,
    pub amount_out_min: U256,
    pub path: Vec<AlloyAddress>,
    pub to: AlloyAddress,
    pub deadline: U256,
}

pub fn encode(p: &Params) -> Vec<u8> {
    swapExactTokensForETHCall {
        amountIn: p.amount_in,
        amountOutMin: p.amount_out_min,
        path: p.path.clone(),
        to: p.to,
        deadline: p.deadline,
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
    let call = swapExactTokensForETHCall::abi_decode(calldata, true)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
    if call.path.len() < 2 {
        return Err(DecodeError::EmptyPath(call.path.len()));
    }
    Ok(Params {
        amount_in: call.amountIn,
        amount_out_min: call.amountOutMin,
        path: call.path,
        to: call.to,
        deadline: call.deadline,
    })
}

pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    tokens: TokenLookup,
}

impl Adapter_ {
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap())],
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
        AdapterId::new("uniswap-v2/swapExactTokensForETH@0.1.0")
    }

    fn match_keys(&self) -> Vec<MatchKey> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| MatchKey::exact(*chain, target.clone(), SELECTOR))
            .collect()
    }

    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let p = decode(&tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        let token_in_addr = Address::from_alloy(*p.path.first().unwrap());
        let recipient_addr = Address::from_alloy(p.to);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = native_eth(tx.chain_id);

        let human_in = shift_decimals(&p.amount_in.to_string(), input_token.decimals);
        let human_min_out = shift_decimals(&p.amount_out_min.to_string(), output_token.decimals);

        Ok(Action::Swap(SwapAction {
            protocol_id: "uniswap-v2".into(),
            actor: tx.from.clone(),
            target: tx.to.clone(),
            value_wei: tx.value_wei.clone(),
            input_token: input_token.clone(),
            output_token: output_token.clone(),
            input_amount: AmountSpec {
                token: input_token,
                raw: p.amount_in.to_string(),
                human: Some(human_in),
                usd: None,
            },
            min_output_amount: Some(AmountSpec {
                token: output_token,
                raw: p.amount_out_min.to_string(),
                human: Some(human_min_out),
                usd: None,
            }),
            recipient: recipient_addr,
            deadline: u64::from_str(&p.deadline.to_string()).ok(),
            fee_bips: Some(30),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> Params {
        Params {
            amount_in: U256::from(200_000_000u64),
            amount_out_min: U256::ZERO,
            path: vec![
                AlloyAddress::from_str("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
                AlloyAddress::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap(),
            ],
            to: AlloyAddress::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            deadline: U256::from(9_999_999_999u64),
        }
    }

    #[test]
    fn round_trip() {
        let p = sample_params();
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }

    #[test]
    fn selector_pin() {
        assert_eq!(SELECTOR, [0x18, 0xcb, 0xaf, 0xe5]);
    }

    #[test]
    fn build_marks_native_eth_as_output_token() {
        let adapter = Adapter_::new();
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data: encode(&sample_params()),
            gas: None,
            nonce: None,
        };
        match adapter.build(&tx).unwrap() {
            Action::Swap(s) => {
                assert_eq!(s.input_token.symbol, "USDT");
                assert!(s.output_token.is_native);
                assert_eq!(s.output_token.symbol, "ETH");
            }
            _ => panic!("expected swap"),
        }
    }
}
