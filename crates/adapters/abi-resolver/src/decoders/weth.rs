use std::str::FromStr as _;

use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall};
use policy_engine::action::Address;

use crate::{
    CallMatchKey, DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder, DecoderError,
    DecoderId,
};

pub const WETH_DEPOSIT_DECODER_ID: &str = "weth/deposit";
pub const WETH_WITHDRAW_DECODER_ID: &str = "weth/withdraw";

pub const WETH_DEPOSIT_SELECTOR: [u8; 4] = [0xd0, 0xe3, 0x0d, 0xb0];
pub const WETH_WITHDRAW_SELECTOR: [u8; 4] = [0x2e, 0x1a, 0x7d, 0x4e];

pub const WETH_MAINNET: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
pub const WETH_BASE: &str = "0x4200000000000000000000000000000000000006";
pub const WETH_OPTIMISM: &str = "0x4200000000000000000000000000000000000006";
pub const WETH_ARBITRUM: &str = "0x82af49447d8a07e3bd95bd0d56f35241523fbab1";
pub const WETH_POLYGON: &str = "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619";

const DEPOSIT_SIGNATURE: &str = "deposit()";
const WITHDRAW_SIGNATURE: &str = "withdraw(uint256)";
const WETH_TARGETS: &[(u64, &str)] = &[
    (1, WETH_MAINNET),
    (8453, WETH_BASE),
    (10, WETH_OPTIMISM),
    (42161, WETH_ARBITRUM),
    (137, WETH_POLYGON),
];

sol! {
    function deposit() external payable;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WethDepositDecoder;

impl WethDepositDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for WethDepositDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(WETH_DEPOSIT_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        weth_match_keys(WETH_DEPOSIT_SELECTOR)
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        depositCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: DEPOSIT_SIGNATURE.to_owned(),
            args: vec![],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WethWithdrawDecoder;

impl WethWithdrawDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for WethWithdrawDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(WETH_WITHDRAW_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        weth_match_keys(WETH_WITHDRAW_SELECTOR)
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        let wad = decode_uint256_arg(calldata, WETH_WITHDRAW_SELECTOR)?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: WITHDRAW_SIGNATURE.to_owned(),
            args: vec![DecodedArg {
                name: "wad".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(wad),
            }],
            nested: vec![],
        })
    }
}

fn decode_uint256_arg(calldata: &[u8], expected_selector: [u8; 4]) -> Result<U256, DecoderError> {
    ensure_selector(calldata, expected_selector)?;
    let args = calldata.get(4..).expect("selector length checked");
    if args.len() != 32 {
        return Err(DecoderError::InvalidCalldata(format!(
            "expected one uint256 argument, got {} bytes",
            args.len()
        )));
    }

    Ok(U256::from_be_slice(args))
}

fn ensure_selector(calldata: &[u8], expected: [u8; 4]) -> Result<(), DecoderError> {
    let selector: [u8; 4] = calldata
        .get(..4)
        .ok_or_else(|| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))?
        .try_into()
        .expect("slice length checked");
    if selector == expected {
        Ok(())
    } else {
        Err(DecoderError::UnsupportedSelector)
    }
}

fn weth_match_keys(selector: [u8; 4]) -> Vec<CallMatchKey> {
    WETH_TARGETS
        .iter()
        .map(|(chain_id, address)| CallMatchKey {
            chain_id: *chain_id,
            to: Address::from_str(address).expect("static WETH address must be valid"),
            selector,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodeContext, DecodedValue, DecoderId};
    use alloy_primitives::U256;
    use policy_engine::action::{Address, DecimalString};

    fn context<'a>(to: &'a Address, value: &'a DecimalString) -> DecodeContext<'a> {
        DecodeContext {
            chain_id: 1,
            to,
            value,
            block_timestamp: None,
        }
    }

    #[test]
    fn decodes_deposit() {
        let decoder = WethDepositDecoder::new();
        let to = Address::from_str(WETH_MAINNET).unwrap();
        let value = DecimalString::from_str("1000000000000000000").unwrap();

        let decoded = decoder
            .decode(&context(&to, &value), &WETH_DEPOSIT_SELECTOR)
            .unwrap();

        assert_eq!(decoded.decoder_id, DecoderId::new(WETH_DEPOSIT_DECODER_ID));
        assert_eq!(decoded.function_signature, "deposit()");
        assert!(decoded.args.is_empty());
        assert!(decoded.nested.is_empty());
    }

    #[test]
    fn decodes_withdraw_with_amount() {
        let decoder = WethWithdrawDecoder::new();
        let amount = U256::from(1_000_000_000_000_000_000_u64);
        let calldata: Vec<u8> = {
            let mut v = Vec::from(WETH_WITHDRAW_SELECTOR);
            v.extend_from_slice(&amount.to_be_bytes::<32>());
            v
        };
        let to = Address::from_str(WETH_MAINNET).unwrap();
        let value = DecimalString::from_str("0").unwrap();

        let decoded = decoder.decode(&context(&to, &value), &calldata).unwrap();

        assert_eq!(decoded.decoder_id, DecoderId::new(WETH_WITHDRAW_DECODER_ID));
        assert_eq!(decoded.function_signature, "withdraw(uint256)");
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 1);
        assert_eq!(decoded.args[0].name, "wad");
        assert_eq!(decoded.args[0].abi_type, "uint256");
        assert_eq!(decoded.args[0].value, DecodedValue::Uint(amount));
    }
}
