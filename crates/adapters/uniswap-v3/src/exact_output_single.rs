//! Uniswap V3 SwapRouter `exactOutputSingle` — single-hop, exact-out. Caller
//! specifies the exact `amountOut` they want, plus a max acceptable
//! `amountInMaximum`.

use crate::common::{shift_decimals, DecodeError, TokenLookup, SWAP_ROUTER_MAINNET};
use alloy_primitives::{
    aliases::{U160, U24},
    Address as AlloyAddress, U256,
};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;
use std::str::FromStr;

sol! {
    #[derive(Debug)]
    struct SolExactOutputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24  fee;
        address recipient;
        uint256 deadline;
        uint256 amountOut;
        uint256 amountInMaximum;
        uint160 sqrtPriceLimitX96;
    }

    function exactOutputSingle(SolExactOutputSingleParams params) external payable returns (uint256 amountIn);
}

pub const SELECTOR: [u8; 4] = exactOutputSingleCall::SELECTOR;

#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub token_in: AlloyAddress,
    pub token_out: AlloyAddress,
    pub fee: u32,
    pub recipient: AlloyAddress,
    pub deadline: U256,
    pub amount_out: U256,
    pub amount_in_maximum: U256,
    pub sqrt_price_limit_x96: U256,
}

pub fn encode(p: &Params) -> Vec<u8> {
    let sqrt_limit_u160 = if p.sqrt_price_limit_x96 > U256::from(U160::MAX) {
        U160::MAX
    } else {
        U160::from_be_slice(&p.sqrt_price_limit_x96.to_be_bytes::<32>()[12..])
    };
    exactOutputSingleCall {
        params: SolExactOutputSingleParams {
            tokenIn: p.token_in,
            tokenOut: p.token_out,
            fee: U24::from(p.fee),
            recipient: p.recipient,
            deadline: p.deadline,
            amountOut: p.amount_out,
            amountInMaximum: p.amount_in_maximum,
            sqrtPriceLimitX96: sqrt_limit_u160,
        },
    }
    .abi_encode()
}

pub fn decode(calldata: &[u8]) -> Result<Params, DecodeError> {
    const NEED: usize = 4 + 8 * 32;
    if calldata.len() < 4 {
        return Err(DecodeError::TooShort {
            need: NEED,
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
    if calldata.len() < NEED {
        return Err(DecodeError::TooShort {
            need: NEED,
            got: calldata.len(),
        });
    }
    let call = exactOutputSingleCall::abi_decode(calldata, true)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
    let fee_u32 = u32::try_from(call.params.fee)
        .map_err(|_| DecodeError::FeeOutOfRange(call.params.fee.to_string()))?;
    Ok(Params {
        token_in: call.params.tokenIn,
        token_out: call.params.tokenOut,
        fee: fee_u32,
        recipient: call.params.recipient,
        deadline: call.params.deadline,
        amount_out: call.params.amountOut,
        amount_in_maximum: call.params.amountInMaximum,
        sqrt_price_limit_x96: U256::from(call.params.sqrtPriceLimitX96),
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
        AdapterId::new("uniswap-v3/exactOutputSingle@0.1.0")
    }

    fn match_keys(&self) -> Vec<MatchKey> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| MatchKey::exact(*chain, target.clone(), SELECTOR))
            .collect()
    }

    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let p = decode(&tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        let token_in_addr = Address::from_alloy(p.token_in);
        let token_out_addr = Address::from_alloy(p.token_out);
        let recipient_addr = Address::from_alloy(p.recipient);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = self.tokens.get(tx.chain_id, &token_out_addr);

        let human_in_max = shift_decimals(&p.amount_in_maximum.to_string(), input_token.decimals);
        let human_out = shift_decimals(&p.amount_out.to_string(), output_token.decimals);

        // For exact-out swaps we surface the (known) `amountOut` as
        // `min_output_amount` and the (worst-case) `amountInMaximum` as
        // `input_amount.raw`. The policy layer reasons about the *guaranteed*
        // values; treating amount_in_max as the conservative input amount
        // makes "max input USD" policies do the right thing.
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
            fee_bips: Some((p.fee / 100) as u32),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> Params {
        Params {
            token_in: AlloyAddress::from_str("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            token_out: AlloyAddress::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")
                .unwrap(),
            fee: 3000,
            recipient: AlloyAddress::from_str("0x1111111111111111111111111111111111111111")
                .unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_out: U256::from(1_000_000_000_000_000_000u64), // 1 WETH
            amount_in_maximum: U256::from(4_000_000_000_u64),     // 4000 USDT max
            sqrt_price_limit_x96: U256::ZERO,
        }
    }

    #[test]
    fn round_trip() {
        let p = sample_params();
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }

    #[test]
    fn selector_pin() {
        assert_eq!(SELECTOR, [0xdb, 0x3e, 0x21, 0x98]);
    }

    #[test]
    fn build_uses_amount_in_maximum_as_input_amount_for_policy_purposes() {
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
                assert_eq!(s.input_token.symbol, "USDT");
                assert_eq!(s.input_amount.raw, "4000000000"); // amountInMaximum
                assert_eq!(s.min_output_amount.unwrap().raw, "1000000000000000000");
                // amountOut
            }
            _ => panic!("expected swap"),
        }
    }
}
