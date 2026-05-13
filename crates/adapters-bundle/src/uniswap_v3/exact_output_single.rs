//! Uniswap V3 `SwapRouter` `exactOutputSingle` — single-hop, exact-out. Caller
//! specifies the exact `amountOut` they want, plus a max acceptable
//! `amountInMaximum`.

#[cfg(test)]
use super::common::SWAP_ROUTER_MAINNET;
use super::common::{
    dex_swap_action, static_adapter_id, swap_router_address, DecodeError, TokenLookup,
};
use alloy_primitives::{
    aliases::{U160, U24},
    Address as AlloyAddress, U256,
};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

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

/// Selector for `exactOutputSingle`.
pub const SELECTOR: [u8; 4] = exactOutputSingleCall::SELECTOR;

/// Decoded `exactOutputSingle` parameters.
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
    /// Exact output amount.
    pub amount_out: U256,
    /// Maximum input amount.
    pub amount_in_maximum: U256,
    /// Optional sqrt price limit.
    pub sqrt_price_limit_x96: U256,
}

/// ABI-encode `exactOutputSingle` calldata.
#[must_use]
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

/// Decode `exactOutputSingle` calldata.
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

/// `TransactionActionAdapter` for `exactOutputSingle`.
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
}

impl Default for Adapter_ {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionActionAdapter for Adapter_ {
    fn id(&self) -> ActionAdapterId {
        static_adapter_id("uniswap-v3/exactOutputSingle@0.1.0")
    }

    fn match_keys(&self) -> Vec<TransactionMatchKey> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| TransactionMatchKey::exact(*chain, target.clone(), SELECTOR))
            .collect()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<LegacyAction, ActionAdapterError> {
        let p = decode(&tx.data).map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
        let token_in_addr = Address::from_alloy(p.token_in);
        let token_out_addr = Address::from_alloy(p.token_out);
        let recipient_addr = Address::from_alloy(p.recipient);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = self.tokens.get(tx.chain_id, &token_out_addr);

        Ok(dex_swap_action(
            tx,
            "uniswap-v3",
            input_token,
            output_token,
            p.amount_in_maximum.to_string(),
            Some(p.amount_out.to_string()),
            &recipient_addr,
            Some(p.fee / 100),
            "exactOutputSingle",
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
        match adapter.build_action(&tx).unwrap() {
            LegacyAction::Dex(d) => {
                assert_eq!(d.facts.protocol_ids, vec!["uniswap-v3"]);
                assert_eq!(d.facts.input_tokens[0].symbol, "USDT");
                assert_eq!(d.facts.output_tokens[0].symbol, "WETH");
                assert_eq!(d.facts.max_fee_bps, Some(30));
                assert_eq!(d.oracle_requirements[0].kind, OracleRequirementKind::Input);
                assert_eq!(d.oracle_requirements[0].raw_amount, "4000000000");
                assert_eq!(
                    d.oracle_requirements[1].kind,
                    OracleRequirementKind::MinOutput
                );
                assert_eq!(d.oracle_requirements[1].raw_amount, "1000000000000000000");
                assert_eq!(d.trace.steps, vec!["exactOutputSingle"]);
            }
            other => panic!("expected dex, got {other:?}"),
        }
    }
}
