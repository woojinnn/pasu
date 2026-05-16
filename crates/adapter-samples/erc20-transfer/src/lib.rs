//! Sample adapter: ERC-20 `transfer(address,uint256)`.
//! Recognises the canonical selector and emits an Action::Other carrying the
//! decoded recipient + amount. This is intentionally minimal; it exercises
//! every part of the SDK without protocol-specific complexity.

use adapter_sdk::prelude::*;
use adapter_sdk::traits::{CallAdapter, Decoder};
use adapter_sdk::types::{DecodedArg, DecodedCall, DecodedValue};
use adapter_sdk::action::Action;
use adapter_sdk_macros::adapter;

const SELECTOR_TRANSFER: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

#[derive(Default)]
#[adapter(
    name = "erc20-transfer",
    version = "0.1.0",
    description = "Canonical ERC-20 transfer decoder",
    capabilities = [decoder, call_adapter],
    applies_to = [
        { chain: 1,     address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
        { chain: 1,     address: "0xdac17f958d2ee523a2206206994597c13d831ec7" },
    ],
)]
pub struct Erc20Transfer;

impl Decoder for Erc20Transfer {
    fn decode_call(&self, ctx: &CallCtx, calldata: &[u8])
        -> Result<DecodedCall, AdapterError>
    {
        if calldata.len() < 4 + 32 + 32 {
            return Err(AdapterError::CalldataTooShort {
                expected: 4 + 32 + 32,
                got: calldata.len(),
            });
        }
        if calldata[..4] != SELECTOR_TRANSFER {
            return Err(AdapterError::UnknownSelector {
                selector: format!("0x{}", hex::encode(&calldata[..4])),
            });
        }
        let mut to_bytes = [0u8; 20];
        to_bytes.copy_from_slice(&calldata[4 + 12..4 + 32]);
        let to = Address(to_bytes);

        let amount = u256_to_decimal(&calldata[4 + 32..4 + 64]);

        Ok(DecodedCall {
            chain_id: ctx.chain_id,
            target: ctx.target,
            selector: Selector(SELECTOR_TRANSFER),
            function: "transfer".into(),
            args: vec![
                DecodedArg { name: "to".into(), value: DecodedValue::Address(to) },
                DecodedArg { name: "amount".into(), value: DecodedValue::Uint(amount) },
            ],
            nested: vec![],
        })
    }
}

impl CallAdapter for Erc20Transfer {
    fn map_to_action(&self, ctx: &CallCtx, decoded: &DecodedCall)
        -> Result<Vec<ActionEnvelope>, AdapterError>
    {
        Ok(vec![ActionEnvelope::new(Action::Other {
            chain_id: ctx.chain_id,
            target: ctx.target,
            decoded: Some(decoded.clone()),
        })])
    }
}

fn u256_to_decimal(bytes: &[u8]) -> String {
    // Convert big-endian 32-byte uint to base-10 string without bringing in
    // a bigint dep. Trivial doubling for ≤256-bit numbers.
    let mut result: Vec<u8> = vec![0];
    for byte in bytes {
        let mut carry: u32 = *byte as u32;
        for digit in &mut result {
            let v = (*digit as u32) * 256 + carry;
            *digit = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            result.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    if result.is_empty() {
        return "0".into();
    }
    result.iter().rev().map(|d| (b'0' + d) as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dummy_ctx() -> CallCtx<'static> {
        static LOG: fn(LogLevel, &str) = |_, _| {};
        static LOOKUP: fn(u64, Address, &[u8]) -> Result<DecodedCall, CtxError> =
            |_, addr, _| Err(CtxError::NotFound { chain: 0, address: addr.to_string() });
        CallCtx {
            chain_id: 1,
            target: Address([0u8; 20]),
            selector: Selector([0; 4]),
            log: &LOG,
            lookup_adapter: &LOOKUP,
        }
    }

    #[test]
    fn decodes_transfer_calldata() {
        // transfer(0x0000…0001, 1000)
        let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
        data.extend(std::iter::repeat(0).take(12));
        data.extend(Address::from_str("0x0000000000000000000000000000000000000001").unwrap().0);
        // amount = 1000
        let mut amt = [0u8; 32];
        amt[30] = 0x03; amt[31] = 0xe8;
        data.extend(&amt);

        let ad = Erc20Transfer;
        let decoded = ad.decode_call(&dummy_ctx(), &data).unwrap();
        assert_eq!(decoded.function, "transfer");
        let amount_arg = &decoded.args[1];
        assert_eq!(amount_arg.value, DecodedValue::Uint("1000".into()));
    }

    #[test]
    fn rejects_short_calldata() {
        let ad = Erc20Transfer;
        let err = ad.decode_call(&dummy_ctx(), &[0xa9]).unwrap_err();
        assert!(matches!(err, AdapterError::CalldataTooShort { .. }));
    }

    #[test]
    fn rejects_wrong_selector() {
        let ad = Erc20Transfer;
        let mut data = vec![0xde, 0xad, 0xbe, 0xef];
        data.extend(std::iter::repeat(0).take(64));
        let err = ad.decode_call(&dummy_ctx(), &data).unwrap_err();
        assert!(matches!(err, AdapterError::UnknownSelector { .. }));
    }
}
