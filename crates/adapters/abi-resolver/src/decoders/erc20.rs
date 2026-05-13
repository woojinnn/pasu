//! ERC-20 entrypoint decoders. Currently covers `approve(address,uint256)`
//! and `transfer(address,uint256)`
//! against a small fixed whitelist of well-known mainnet token addresses
//! (USDT, USDC, DAI, WETH). Wildcard-`to` registry support is a follow-up;
//! until then a Decoder must enumerate every (chain, token, selector) it
//! handles.

use std::str::FromStr as _;

use alloy_primitives::Address as AlloyAddress;
use alloy_sol_types::{sol, SolCall};
use policy_engine::action::Address;

use crate::{
    CallMatchKey, DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder, DecoderError,
    DecoderId,
};

pub const ERC20_APPROVE_DECODER_ID: &str = "erc20/approve";
pub const ERC20_TRANSFER_DECODER_ID: &str = "erc20/transfer";

/// `approve(address,uint256)` selector. Same across every ERC-20.
pub const APPROVE_SELECTOR: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];
/// `transfer(address,uint256)` selector. Same across every ERC-20.
pub const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

const APPROVE_SIGNATURE: &str = "approve(address,uint256)";
const TRANSFER_SIGNATURE: &str = "transfer(address,uint256)";

sol! {
    function approve(address spender, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
}

/// Mainnet ERC-20 addresses whose `approve` calldata we know how to decode
/// out of the box. Listing them explicitly here lets the registry stay
/// strict-match-key (no wildcard `to`) while still demonstrating ERC-20
/// flow end-to-end on the golden fixture set.
const KNOWN_MAINNET_TOKENS: &[&str] = &[
    "0xdac17f958d2ee523a2206206994597c13d831ec7", // USDT
    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC
    "0x6b175474e89094c44da98b954eedeac495271d0f", // DAI
    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
];

#[derive(Debug, Clone, Copy, Default)]
pub struct Erc20ApproveDecoder;

impl Erc20ApproveDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for Erc20ApproveDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(ERC20_APPROVE_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        KNOWN_MAINNET_TOKENS
            .iter()
            .filter_map(|addr_s| Address::from_str(addr_s).ok())
            .map(|to| CallMatchKey {
                chain_id: 1,
                to,
                selector: APPROVE_SELECTOR,
            })
            .collect()
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        let call = approveCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;
        let spender = decode_address(&call.spender)?;
        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: APPROVE_SIGNATURE.to_string(),
            args: vec![
                DecodedArg {
                    name: "spender".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(spender),
                },
                DecodedArg {
                    name: "amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(call.amount),
                },
            ],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Erc20TransferDecoder;

impl Erc20TransferDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for Erc20TransferDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(ERC20_TRANSFER_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        KNOWN_MAINNET_TOKENS
            .iter()
            .filter_map(|addr_s| Address::from_str(addr_s).ok())
            .map(|to| CallMatchKey {
                chain_id: 1,
                to,
                selector: TRANSFER_SELECTOR,
            })
            .collect()
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        let call = transferCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;
        let to = decode_address(&call.to)?;
        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: TRANSFER_SIGNATURE.to_string(),
            args: vec![
                DecodedArg {
                    name: "to".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(to),
                },
                DecodedArg {
                    name: "amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(call.amount),
                },
            ],
            nested: vec![],
        })
    }
}

fn decode_address(value: &AlloyAddress) -> Result<Address, DecoderError> {
    Address::from_str(&format!("{value:#x}"))
        .map_err(|e| DecoderError::Internal(anyhow::anyhow!("address: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_keys_cover_known_tokens() {
        let adapter = Erc20ApproveDecoder::new();
        let keys = adapter.match_keys();
        assert_eq!(keys.len(), KNOWN_MAINNET_TOKENS.len());
        assert!(keys.iter().all(|k| k.selector == APPROVE_SELECTOR));
        assert!(keys.iter().all(|k| k.chain_id == 1));
    }

    #[test]
    fn transfer_match_keys_cover_known_tokens() {
        let adapter = Erc20TransferDecoder::new();
        let keys = adapter.match_keys();
        assert_eq!(keys.len(), KNOWN_MAINNET_TOKENS.len());
        assert!(keys.iter().all(|k| k.selector == TRANSFER_SELECTOR));
        assert!(keys.iter().all(|k| k.chain_id == 1));
    }

    #[test]
    fn decodes_unlimited_approve() {
        let adapter = Erc20ApproveDecoder::new();
        // approve(0x1111..., uint256::MAX)
        let calldata: Vec<u8> = {
            let mut v = Vec::from(APPROVE_SELECTOR);
            v.extend_from_slice(&[0u8; 12]);
            v.extend_from_slice(&[0x11; 20]);
            v.extend_from_slice(&[0xff; 32]);
            v
        };
        let to = Address::from_str(KNOWN_MAINNET_TOKENS[0]).unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let ctx = DecodeContext {
            chain_id: 1,
            to: &to,
            value: &value,
            block_timestamp: None,
        };
        let decoded = adapter.decode(&ctx, &calldata).unwrap();
        assert_eq!(decoded.decoder_id.as_str(), ERC20_APPROVE_DECODER_ID);
        assert_eq!(decoded.args.len(), 2);
        assert_eq!(decoded.args[0].name, "spender");
        assert_eq!(decoded.args[1].name, "amount");
    }

    #[test]
    fn decodes_transfer() {
        let adapter = Erc20TransferDecoder::new();
        // transfer(0x1111..., 1_000_000_000_000)
        let calldata: Vec<u8> = {
            let mut v = Vec::from(TRANSFER_SELECTOR);
            v.extend_from_slice(&[0u8; 12]);
            v.extend_from_slice(&[0x11; 20]);
            v.extend_from_slice(
                &alloy_primitives::U256::from(1_000_000_000_000_u64).to_be_bytes::<32>(),
            );
            v
        };
        let token = Address::from_str(KNOWN_MAINNET_TOKENS[0]).unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let ctx = DecodeContext {
            chain_id: 1,
            to: &token,
            value: &value,
            block_timestamp: None,
        };
        let decoded = adapter.decode(&ctx, &calldata).unwrap();
        assert_eq!(decoded.decoder_id.as_str(), ERC20_TRANSFER_DECODER_ID);
        assert_eq!(decoded.args.len(), 2);
        assert_eq!(decoded.args[0].name, "to");
        assert_eq!(decoded.args[1].name, "amount");
    }
}
