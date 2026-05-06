//! Uniswap V3 `SwapRouter` `exactInput` — multi-hop, exact-in. The token route
//! arrives as a packed `bytes path = [tokenA][fee0][tokenB][fee1]...[tokenN]`,
//! decoded by `common::decode_v3_path`.

#[cfg(test)]
use crate::common::SWAP_ROUTER_MAINNET;
use crate::common::{
    decode_v3_path, dex_swap_action, path_endpoints, static_adapter_id, swap_router_address,
    DecodeError, TokenLookup,
};
use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

sol! {
    #[derive(Debug)]
    struct SolExactInputParams {
        bytes   path;
        address recipient;
        uint256 deadline;
        uint256 amountIn;
        uint256 amountOutMinimum;
    }

    function exactInput(SolExactInputParams params) external payable returns (uint256 amountOut);
}

/// Selector for `exactInput`.
pub const SELECTOR: [u8; 4] = exactInputCall::SELECTOR;

/// Decoded `exactInput` parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Packed V3 path from input token to output token.
    pub path: Vec<u8>,
    /// Recipient address.
    pub recipient: AlloyAddress,
    /// Swap deadline.
    pub deadline: U256,
    /// Exact input amount.
    pub amount_in: U256,
    /// Minimum acceptable output amount.
    pub amount_out_minimum: U256,
}

/// ABI-encode `exactInput` calldata.
#[must_use]
pub fn encode(p: &Params) -> Vec<u8> {
    exactInputCall {
        params: SolExactInputParams {
            path: p.path.clone().into(),
            recipient: p.recipient,
            deadline: p.deadline,
            amountIn: p.amount_in,
            amountOutMinimum: p.amount_out_minimum,
        },
    }
    .abi_encode()
}

/// Decode `exactInput` calldata.
///
/// # Errors
///
/// Returns an error when calldata is too short, has the wrong selector, or
/// fails ABI decoding.
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
    let call = exactInputCall::abi_decode(calldata, true)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;

    Ok(Params {
        path: call.params.path.to_vec(),
        recipient: call.params.recipient,
        deadline: call.params.deadline,
        amount_in: call.params.amountIn,
        amount_out_minimum: call.params.amountOutMinimum,
    })
}

/// Adapter for `exactInput`.
#[derive(Debug)]
pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    tokens: TokenLookup,
}

impl Adapter_ {
    /// Construct an adapter with mainnet `SwapRouter` and default token metadata.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, swap_router_address())],
            tokens: TokenLookup::with_mainnet_defaults(),
        }
    }

    /// Returns this adapter after adding `token` to its lookup.
    #[must_use]
    pub fn with_token(mut self, token: Token) -> Self {
        self.tokens.add(token);
        self
    }
}

impl Default for Adapter_ {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for Adapter_ {
    fn id(&self) -> AdapterId {
        static_adapter_id("uniswap-v3/exactInput@0.1.0")
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

        let (token_in, token_out) = path_endpoints(&alloy_tokens)?;
        let token_in_addr = Address::from_alloy(token_in);
        let token_out_addr = Address::from_alloy(token_out);
        let recipient_addr = Address::from_alloy(p.recipient);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = self.tokens.get(tx.chain_id, &token_out_addr);

        let max_fee_bps = fees.iter().max().map(|fee| fee / 100);

        Ok(dex_swap_action(
            tx,
            "uniswap-v3",
            input_token,
            output_token,
            p.amount_in.to_string(),
            Some(p.amount_out_minimum.to_string()),
            &recipient_addr,
            max_fee_bps,
            "exactInput",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn build_path(token_a: &str, fee: u32, token_b: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(43);
        out.extend_from_slice(AlloyAddress::from_str(token_a).unwrap().as_slice());
        out.extend_from_slice(&fee.to_be_bytes()[1..4]); // 3-byte fee
        out.extend_from_slice(AlloyAddress::from_str(token_b).unwrap().as_slice());
        out
    }

    fn sample_params() -> Params {
        Params {
            path: build_path(
                "0xdAC17F958D2ee523a2206206994597C13D831ec7", // USDT
                3000,
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", // WETH
            ),
            recipient: AlloyAddress::from_str("0x1111111111111111111111111111111111111111")
                .unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_in: U256::from(200_000_000u64),
            amount_out_minimum: U256::ZERO,
        }
    }

    #[test]
    fn round_trip() {
        let p = sample_params();
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }

    #[test]
    fn selector_pin() {
        assert_eq!(SELECTOR, [0xc0, 0x4b, 0x8d, 0x59]);
    }

    #[test]
    fn build_emits_dex_with_correct_input_output_tokens() {
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
            Action::Dex(d) => {
                assert_eq!(d.facts.protocol_ids, vec!["uniswap-v3"]);
                assert_eq!(d.facts.input_tokens[0].symbol, "USDT");
                assert_eq!(d.facts.output_tokens[0].symbol, "WETH");
                assert_eq!(d.facts.max_fee_bps, Some(30));
                assert_eq!(d.oracle_requirements[0].raw_amount, "200000000");
                assert_eq!(d.oracle_requirements[1].raw_amount, "0");
                assert_eq!(d.trace.steps, vec!["exactInput"]);
            }
            Action::Other(_) => panic!("expected dex"),
        }
    }

    #[test]
    fn multi_hop_path_uses_max_fee() {
        let mut path = Vec::new();
        path.extend_from_slice(
            AlloyAddress::from_str("0xdAC17F958D2ee523a2206206994597C13D831ec7")
                .unwrap()
                .as_slice(),
        ); // USDT
        path.extend_from_slice(&500u32.to_be_bytes()[1..4]); // 5 bp
        path.extend_from_slice(
            AlloyAddress::from_str("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48")
                .unwrap()
                .as_slice(),
        ); // USDC
        path.extend_from_slice(&3000u32.to_be_bytes()[1..4]); // 30 bp
        path.extend_from_slice(
            AlloyAddress::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")
                .unwrap()
                .as_slice(),
        ); // WETH

        let p = Params {
            path,
            recipient: AlloyAddress::from_str("0x1111111111111111111111111111111111111111")
                .unwrap(),
            deadline: U256::from(1u64),
            amount_in: U256::from(1_000_000u64),
            amount_out_minimum: U256::ZERO,
        };

        let adapter = Adapter_::new();
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
            Action::Dex(d) => {
                assert_eq!(d.facts.input_tokens[0].symbol, "USDT");
                assert_eq!(d.facts.output_tokens[0].symbol, "WETH");
                assert_eq!(d.facts.max_fee_bps, Some(30));
            }
            Action::Other(_) => panic!("expected dex"),
        }
    }
}
