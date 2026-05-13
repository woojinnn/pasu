//! Uniswap V2 Router02 `swapTokensForExactETH(uint256 amountOut,
//! uint256 amountInMax, address[] path, address to, uint256 deadline)`.
//! Tokens → exact ETH amount, with `amountInMax` ceiling.

#[cfg(test)]
use super::common::UNISWAP_V2_ROUTER_MAINNET;
use super::common::{
    dex_swap_action, native_eth, path_endpoints, router_address, static_adapter_id, DecodeError,
    TokenLookup,
};
use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::{sol, SolCall};
use policy_engine::prelude::*;

sol! {
    function swapTokensForExactETH(
        uint256 amountOut,
        uint256 amountInMax,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[] amounts);
}

/// Selector for `swapTokensForExactETH`.
pub const SELECTOR: [u8; 4] = swapTokensForExactETHCall::SELECTOR;

/// Decoded `swapTokensForExactETH` parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Exact ETH output amount.
    pub amount_out: U256,
    /// Maximum input token amount.
    pub amount_in_max: U256,
    /// Ordered token path from input token to WETH.
    pub path: Vec<AlloyAddress>,
    /// Recipient address.
    pub to: AlloyAddress,
    /// Swap deadline.
    pub deadline: U256,
}

/// ABI-encode `swapTokensForExactETH` calldata.
#[must_use]
pub fn encode(p: &Params) -> Vec<u8> {
    swapTokensForExactETHCall {
        amountOut: p.amount_out,
        amountInMax: p.amount_in_max,
        path: p.path.clone(),
        to: p.to,
        deadline: p.deadline,
    }
    .abi_encode()
}

/// Decode `swapTokensForExactETH` calldata.
///
/// # Errors
///
/// Returns an error when calldata is too short, has the wrong selector, fails
/// ABI decoding, or contains a path with fewer than two tokens.
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
    let call = swapTokensForExactETHCall::abi_decode(calldata, true)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
    if call.path.len() < 2 {
        return Err(DecodeError::EmptyPath(call.path.len()));
    }
    Ok(Params {
        amount_out: call.amountOut,
        amount_in_max: call.amountInMax,
        path: call.path,
        to: call.to,
        deadline: call.deadline,
    })
}

/// `TransactionActionAdapter` for `swapTokensForExactETH`.
#[derive(Debug)]
pub struct Adapter_ {
    chain_targets: Vec<(ChainId, Address)>,
    tokens: TokenLookup,
}

impl Adapter_ {
    /// Construct an adapter with mainnet Router02 and default token metadata.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain_targets: vec![(1, router_address())],
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
        static_adapter_id("uniswap-v2/swapTokensForExactETH@0.1.0")
    }

    fn match_keys(&self) -> Vec<TransactionMatchKey> {
        self.chain_targets
            .iter()
            .map(|(chain, target)| TransactionMatchKey::exact(*chain, target.clone(), SELECTOR))
            .collect()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<LegacyAction, ActionAdapterError> {
        let p = decode(&tx.data).map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
        let (token_in, _) = path_endpoints(&p.path)?;
        let token_in_addr = Address::from_alloy(token_in);
        let recipient_addr = Address::from_alloy(p.to);

        let input_token = self.tokens.get(tx.chain_id, &token_in_addr);
        let output_token = native_eth(tx.chain_id);

        Ok(dex_swap_action(
            tx,
            "uniswap-v2",
            input_token,
            output_token,
            p.amount_in_max.to_string(),
            Some(p.amount_out.to_string()),
            &recipient_addr,
            Some(30),
            "uniswap-v2/swapTokensForExactETH",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn sample_params() -> Params {
        Params {
            amount_out: U256::from(1_000_000_000_000_000_000u64),
            amount_in_max: U256::from(4_000_000_000_u64),
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
        assert_eq!(SELECTOR, [0x4a, 0x25, 0xd9, 0x4a]);
    }

    #[test]
    fn build_uses_amount_in_max_for_token_input() {
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
        match adapter.build_action(&tx).unwrap() {
            LegacyAction::Dex(d) => {
                assert_eq!(d.facts.protocol_ids, vec!["uniswap-v2".to_string()]);
                assert_eq!(d.facts.input_tokens[0].symbol, "USDT");
                assert!(d.facts.output_tokens[0].is_native);
                assert_eq!(d.facts.output_tokens[0].symbol, "ETH");
                assert_eq!(d.facts.max_fee_bps, Some(30));
                assert!(!d.facts.has_zero_min_output);
                assert_eq!(d.oracle_requirements[0].kind, OracleRequirementKind::Input);
                assert_eq!(d.oracle_requirements[0].raw_amount, "4000000000");
                assert_eq!(
                    d.oracle_requirements[1].kind,
                    OracleRequirementKind::MinOutput
                );
                assert_eq!(d.oracle_requirements[1].raw_amount, "1000000000000000000");
            }
            other => panic!("expected dex, got {other:?}"),
        }
    }
}
