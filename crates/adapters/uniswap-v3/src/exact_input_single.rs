//! Uniswap V3 `SwapRouter` `exactInputSingle`.
//!
//! This module contains the `sol!` declaration, encode/decode pair, and adapter
//! implementation for a matching `TransactionRequest`.
//!
//! Mainnet `SwapRouter` (the original V3 router with `deadline` in the params
//! struct) lives at `0xE592427A0AEce92De3Edee1F18E0157C05861564`.

#[cfg(test)]
use crate::common::SWAP_ROUTER_MAINNET;
use crate::common::{dex_swap_action, swap_router_address, DecodeError, TokenLookup};
use alloy_primitives::{
    aliases::{U160, U24},
    Address as AlloyAddress, U256,
};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

sol! {
    /// Solidity declaration of the V3 router's `exactInputSingle` params,
    /// used by `alloy_sol_types` to derive `abi_encode` / `abi_decode`.
    #[derive(Debug)]
    struct SolExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24  fee;
        address recipient;
        uint256 deadline;
        uint256 amountIn;
        uint256 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
    }

    function exactInputSingle(SolExactInputSingleParams params) external payable returns (uint256 amountOut);
}

/// Selector for `exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))`.
/// Computed by the `sol!` macro from the Solidity signature above (= `0x414bf389`).
pub const SELECTOR: [u8; 4] = exactInputSingleCall::SELECTOR;

/// Public-facing decoded parameters.
///
/// Callers do not see alloy's typed `Uint<24>` / `Uint<160>`. The
/// `sqrt_price_limit_x96` value is widened to `U256` and narrowed on encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Input token address.
    pub token_in: AlloyAddress,
    /// Output token address.
    pub token_out: AlloyAddress,
    /// Pool fee tier in hundredths of a bip.
    pub fee: u32,
    /// Recipient address.
    pub recipient: AlloyAddress,
    /// Swap deadline.
    pub deadline: U256,
    /// Exact input amount.
    pub amount_in: U256,
    /// Minimum acceptable output amount.
    pub amount_out_minimum: U256,
    /// Optional sqrt price limit.
    pub sqrt_price_limit_x96: U256,
}

/// ABI-encode the call (selector + ABI-encoded tuple).
#[must_use]
pub fn encode(p: &Params) -> Vec<u8> {
    // U160::saturating semantics: input is U256 wide for API ergonomics, but
    // the on-chain field is uint160. A larger value is a programming mistake;
    // we silently saturate. For realistic inputs this branch is never taken.
    let sqrt_limit_u160 = if p.sqrt_price_limit_x96 > U256::from(U160::MAX) {
        U160::MAX
    } else {
        U160::from_be_slice(&p.sqrt_price_limit_x96.to_be_bytes::<32>()[12..])
    };

    let call = exactInputSingleCall {
        params: SolExactInputSingleParams {
            tokenIn: p.token_in,
            tokenOut: p.token_out,
            fee: U24::from(p.fee),
            recipient: p.recipient,
            deadline: p.deadline,
            amountIn: p.amount_in,
            amountOutMinimum: p.amount_out_minimum,
            sqrtPriceLimitX96: sqrt_limit_u160,
        },
    };
    call.abi_encode()
}

/// ABI-decode calldata that begins with the `exactInputSingle` selector.
///
/// # Errors
///
/// Returns an error when calldata is too short, has the wrong selector, fails
/// ABI decoding, or contains an out-of-range fee.
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

    let call = exactInputSingleCall::abi_decode(calldata, true)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;

    let fee_u32 = u32::try_from(call.params.fee)
        .map_err(|_| DecodeError::FeeOutOfRange(call.params.fee.to_string()))?;

    Ok(Params {
        token_in: call.params.tokenIn,
        token_out: call.params.tokenOut,
        fee: fee_u32,
        recipient: call.params.recipient,
        deadline: call.params.deadline,
        amount_in: call.params.amountIn,
        amount_out_minimum: call.params.amountOutMinimum,
        sqrt_price_limit_x96: U256::from(call.params.sqrtPriceLimitX96),
    })
}

/// Adapter for `exactInputSingle`. Holds chain-target list + token lookup.
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

impl TypedAdapter for Adapter_ {
    const ADAPTER_ID: &'static str = "uniswap-v3/exactInputSingle@0.1.0";
    const PROTOCOL_ID: &'static str = "uniswap-v3";
    const KIND: AdapterKind = AdapterKind::Function;
    const FUNCTIONS: &'static [SolidityFunctionSpec] = &[SolidityFunctionSpec::new(
        "exactInputSingle",
        "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
        SELECTOR,
    )];
    const EMITTED_ACTIONS: &'static [ActionKind] = &[ActionKind::Dex];

    fn contract_targets(&self) -> Vec<ContractTarget> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| ContractTarget::new(*chain, target.clone()))
            .collect()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let p = decode(&tx.data).map_err(|e| AdapterError::BadCalldata(e.to_string()))?;

        let token_in_addr = Address::from_alloy(p.token_in);
        let token_out_addr = Address::from_alloy(p.token_out);
        let recipient_addr = Address::from_alloy(p.recipient);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = self.tokens.get(tx.chain_id, &token_out_addr);

        Ok(dex_swap_action(
            tx,
            Self::PROTOCOL_ID,
            input_token,
            output_token,
            p.amount_in.to_string(),
            Some(p.amount_out_minimum.to_string()),
            &recipient_addr,
            Some(p.fee / 100),
            "exactInputSingle",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn sample_params() -> Params {
        Params {
            token_in: AlloyAddress::from_str("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            token_out: AlloyAddress::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")
                .unwrap(),
            fee: 3000,
            recipient: AlloyAddress::from_str("0x1111111111111111111111111111111111111111")
                .unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_in: U256::from(200_000_000u64),
            amount_out_minimum: U256::ZERO,
            sqrt_price_limit_x96: U256::ZERO,
        }
    }

    fn build_tx(amount_in: U256) -> TransactionRequest {
        let mut p = sample_params();
        p.amount_in = amount_in;
        TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data: encode(&p),
            gas: None,
            nonce: None,
        }
    }

    // -- encode/decode --

    #[test]
    fn round_trip_encode_decode() {
        let p = sample_params();
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }

    #[test]
    fn decode_rejects_bad_selector() {
        let mut calldata = encode(&sample_params());
        calldata[0] = 0xff;
        assert!(matches!(
            decode(&calldata).unwrap_err(),
            DecodeError::BadSelector { .. }
        ));
    }

    #[test]
    fn decode_rejects_short_calldata() {
        assert!(matches!(
            decode(&[0x41, 0x4b, 0xf3, 0x89, 0x00, 0x00]).unwrap_err(),
            DecodeError::TooShort { .. }
        ));
    }

    #[test]
    fn decoded_amount_in_is_200_usdt() {
        let decoded = decode(&encode(&sample_params())).unwrap();
        assert_eq!(decoded.amount_in, U256::from(200_000_000u64));
    }

    #[test]
    fn decoded_fee_tier_is_3000() {
        let decoded = decode(&encode(&sample_params())).unwrap();
        assert_eq!(decoded.fee, 3000);
    }

    #[test]
    fn decoded_addresses_round_trip() {
        let p = sample_params();
        let decoded = decode(&encode(&p)).unwrap();
        assert_eq!(decoded.token_in, p.token_in);
        assert_eq!(decoded.token_out, p.token_out);
        assert_eq!(decoded.recipient, p.recipient);
    }

    #[test]
    fn selector_constant_is_known() {
        assert_eq!(SELECTOR, [0x41, 0x4b, 0xf3, 0x89]);
    }

    #[test]
    fn sqrt_price_limit_above_u160_max_saturates() {
        let mut p = sample_params();
        p.sqrt_price_limit_x96 = U256::MAX;
        let decoded = decode(&encode(&p)).unwrap();
        assert_eq!(decoded.sqrt_price_limit_x96, U256::from(U160::MAX));
    }

    // -- Adapter impl --

    #[test]
    fn match_keys_target_swap_router_on_mainnet() {
        let adapter = Adapter_::new();
        let keys = adapter.match_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].chain_id, 1);
        assert_eq!(keys[0].selector, SELECTOR);
        assert_eq!(
            keys[0].to.as_ref().unwrap().as_str().to_lowercase(),
            SWAP_ROUTER_MAINNET.to_lowercase()
        );
    }

    #[test]
    fn build_emits_dex_action_with_known_tokens() {
        let adapter = Adapter_::new();
        let action = adapter
            .build(&build_tx(U256::from(200_000_000u64)))
            .unwrap();
        match action {
            Action::Dex(d) => {
                assert_eq!(d.facts.protocol_ids, vec!["uniswap-v3"]);
                assert_eq!(d.facts.input_tokens[0].symbol, "USDT");
                assert_eq!(d.facts.output_tokens[0].symbol, "WETH");
                assert_eq!(d.facts.max_fee_bps, Some(30));
                assert!(d.facts.has_zero_min_output);
                assert!(d.facts.has_external_recipient);
                assert_eq!(d.oracle_requirements.len(), 2);
                assert_eq!(d.oracle_requirements[0].kind, OracleRequirementKind::Input);
                assert_eq!(d.oracle_requirements[0].raw_amount, "200000000");
                assert_eq!(
                    d.oracle_requirements[1].kind,
                    OracleRequirementKind::MinOutput
                );
                assert_eq!(d.oracle_requirements[1].raw_amount, "0");
                assert_eq!(d.trace.steps, vec!["exactInputSingle"]);
            }
            other => panic!("expected dex action, got {other:?}"),
        }
    }

    #[test]
    fn build_with_unknown_token_falls_back_to_unknown_symbol() {
        let adapter = Adapter_::new();
        let mut p = sample_params();
        p.token_in = AlloyAddress::from_str("0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap();
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
            Action::Dex(d) => assert_eq!(d.facts.input_tokens[0].symbol, "UNKNOWN"),
            other => panic!("expected dex, got {other:?}"),
        }
    }

    #[test]
    fn build_fails_on_garbage_calldata() {
        let adapter = Adapter_::new();
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data: vec![0x41, 0x4b, 0xf3, 0x89, 0x00], // truncated
            gas: None,
            nonce: None,
        };
        assert!(adapter.build(&tx).is_err());
    }
}
