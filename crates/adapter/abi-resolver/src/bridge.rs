//! Bridge: convert legacy `decode::DecodedCall` (DynSolValue-based) to the new
//! `decoder::DecodedCall` (DecodedValue-based) so legacy Sourcify-driven
//! decodes can feed the new `Mapper` trait pipeline.
//!
//! Used as the fallback path in `request-router::route_request` when no
//! per-function `Decoder` matches.

use std::str::FromStr as _;

use alloy_dyn_abi::DynSolValue;
use alloy_primitives::Address as AlloyAddress;
use policy_engine::action::Address;

use crate::decode::{DecodedArg as LegacyArg, DecodedCall as LegacyCall};
use crate::decoder::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use crate::ids::{
    APPROVE_SELECTOR, ERC20_APPROVE_DECODER_ID, ERC20_TRANSFER_DECODER_ID,
    ERC20_TRANSFER_FROM_DECODER_ID, SET_APPROVAL_FOR_ALL_DECODER_ID,
    SET_APPROVAL_FOR_ALL_SELECTOR, TRANSFER_FROM_SELECTOR, TRANSFER_SELECTOR,
};
use crate::ids::{
    SR02_EXACT_INPUT_DECODER_ID, SR02_EXACT_INPUT_SELECTOR, SR02_EXACT_INPUT_SINGLE_DECODER_ID,
    SR02_EXACT_INPUT_SINGLE_SELECTOR, SR02_EXACT_OUTPUT_DECODER_ID, SR02_EXACT_OUTPUT_SELECTOR,
    SR02_EXACT_OUTPUT_SINGLE_DECODER_ID, SR02_EXACT_OUTPUT_SINGLE_SELECTOR,
};
use crate::ids::{
    SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID, SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR,
    SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID, SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID,
    SWAP_EXACT_ETH_FOR_TOKENS_FOT_SELECTOR, SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR,
    SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID, SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_ETH_FOT_SELECTOR, SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR,
    SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_SELECTOR, SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR,
    SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID, SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR,
    SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID, SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR,
};
use crate::ids::{
    EXACT_INPUT_SELECTOR, EXACT_INPUT_SINGLE_SELECTOR, EXACT_OUTPUT_DECODER_ID,
    EXACT_OUTPUT_SELECTOR, EXACT_OUTPUT_SINGLE_DECODER_ID, EXACT_OUTPUT_SINGLE_SELECTOR,
    UNISWAP_V3_DECODER_ID,
};
use crate::ids::{
    WETH_DEPOSIT_DECODER_ID, WETH_DEPOSIT_SELECTOR, WETH_WITHDRAW_DECODER_ID,
    WETH_WITHDRAW_SELECTOR,
};

/// Convert a legacy `DecodedCall` (Sourcify-decoded) into the new shape so it
/// can be dispatched against the new `MapperRegistry`.
///
/// `selector` is the 4-byte selector extracted from the original calldata.
/// We use it to look up a known `decoder_id` so existing mappers (keyed by
/// decoder_id) match without modification. Unknown selectors get a synthetic
/// `fallback/0x<selector>` ID — these will not match any registered mapper
/// today, which is the expected behaviour for a function we don't know yet.
pub fn convert_legacy_call(
    legacy: LegacyCall,
    selector: [u8; 4],
) -> Result<DecodedCall, BridgeError> {
    let decoder_id = decoder_id_for_selector(selector)
        .map(DecoderId::new)
        .unwrap_or_else(|| fallback_decoder_id(selector));

    // V3 / SR02 style: function takes a single `params` struct. Sourcify hands
    // it back as one top-level `Tuple` arg with field metadata on
    // `components`. The new-pipeline mappers were written against the
    // `sol!`-flattened layout (one top-level arg per struct field), so we
    // flatten here to keep the mapper API stable.
    let args = if legacy.args.len() == 1
        && matches!(legacy.args[0].value, DynSolValue::Tuple(_))
        && !legacy.args[0].components.is_empty()
    {
        let only = legacy.args.into_iter().next().expect("len == 1 checked");
        flatten_tuple_arg(only)?
    } else {
        legacy
            .args
            .into_iter()
            .map(convert_arg)
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(DecodedCall {
        decoder_id,
        function_signature: legacy.signature,
        args,
        nested: vec![],
    })
}

/// Flatten a `Tuple`-valued top-level argument into one new-pipeline
/// [`DecodedArg`] per field, using `components` for field names. The function
/// signature for `legacy.sol_type` is expected to be a tuple type like
/// `(address,address,uint24,...)`.
fn flatten_tuple_arg(arg: LegacyArg) -> Result<Vec<DecodedArg>, BridgeError> {
    let DynSolValue::Tuple(items) = arg.value else {
        return Err(BridgeError::UnsupportedValue(
            "flatten_tuple_arg called on non-tuple".to_string(),
        ));
    };
    if items.len() != arg.components.len() {
        // Defensive: mismatch between tuple values and parameter metadata.
        // Fall back to keeping it as a single nested Tuple arg under the
        // original parameter name so the mapper at least sees something.
        let value = convert_value(DynSolValue::Tuple(items))?;
        return Ok(vec![DecodedArg {
            name: arg.name,
            abi_type: arg.sol_type,
            value,
        }]);
    }
    let mut out = Vec::with_capacity(items.len());
    for (value, component) in items.into_iter().zip(arg.components.into_iter()) {
        let name = if component.name.is_empty() {
            format!("arg{}", out.len())
        } else {
            component.name
        };
        out.push(DecodedArg {
            name,
            abi_type: component.ty,
            value: convert_value(value)?,
        });
    }
    Ok(out)
}

fn convert_arg(legacy: LegacyArg) -> Result<DecodedArg, BridgeError> {
    Ok(DecodedArg {
        name: legacy.name,
        abi_type: legacy.sol_type,
        value: convert_value(legacy.value)?,
    })
}

fn convert_value(value: DynSolValue) -> Result<DecodedValue, BridgeError> {
    Ok(match value {
        DynSolValue::Address(addr) => DecodedValue::Address(address_to_policy(addr)?),
        DynSolValue::Uint(v, _) => DecodedValue::Uint(v),
        DynSolValue::Int(v, _) => DecodedValue::Int(v),
        DynSolValue::Bool(b) => DecodedValue::Bool(b),
        DynSolValue::Bytes(b) => DecodedValue::Bytes(b),
        DynSolValue::FixedBytes(word, len) => DecodedValue::Bytes(word.as_slice()[..len].to_vec()),
        DynSolValue::String(s) => DecodedValue::String(s),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => DecodedValue::Array(
            items
                .into_iter()
                .map(convert_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DynSolValue::Tuple(items) => DecodedValue::Tuple(
            items
                .into_iter()
                .map(convert_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DynSolValue::Function(_) => return Err(BridgeError::UnsupportedValue("function".into())),
    })
}

fn address_to_policy(addr: AlloyAddress) -> Result<Address, BridgeError> {
    Address::from_str(&format!("0x{}", hex::encode(addr.0)))
        .map_err(|e| BridgeError::AddressFormat(e.to_string()))
}

fn fallback_decoder_id(selector: [u8; 4]) -> DecoderId {
    DecoderId::new(format!("fallback/0x{}", hex::encode(selector)))
}

/// Lookup table: selector → existing `decoder_id`. Mirrors the IDs declared
/// per-function in `crates/adapters/abi-resolver/src/decoders/*.rs`. Keeping
/// this central means we only need to update one place when a new per-function
/// decoder lands.
pub fn decoder_id_for_selector(selector: [u8; 4]) -> Option<&'static str> {
    match selector {
        // ── erc20 ─────────────────────────────────────────────────────────────
        APPROVE_SELECTOR => Some(ERC20_APPROVE_DECODER_ID),
        TRANSFER_SELECTOR => Some(ERC20_TRANSFER_DECODER_ID),
        TRANSFER_FROM_SELECTOR => Some(ERC20_TRANSFER_FROM_DECODER_ID),
        SET_APPROVAL_FOR_ALL_SELECTOR => Some(SET_APPROVAL_FOR_ALL_DECODER_ID),

        // ── uniswap v2 ────────────────────────────────────────────────────────
        SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR => Some(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID),
        SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR => Some(SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID),
        SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR => Some(SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID),
        SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR => Some(SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID),
        SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR => Some(SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID),
        SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR => Some(SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID),
        SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_SELECTOR => {
            Some(SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID)
        }
        SWAP_EXACT_ETH_FOR_TOKENS_FOT_SELECTOR => Some(SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID),
        SWAP_EXACT_TOKENS_FOR_ETH_FOT_SELECTOR => Some(SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID),

        // ── uniswap v3 ────────────────────────────────────────────────────────
        // ExactInputSingle / ExactInput both share UNISWAP_V3_DECODER_ID in the
        // current new-pipeline mapper layout. The mapper distinguishes by
        // selector internally.
        EXACT_INPUT_SINGLE_SELECTOR | EXACT_INPUT_SELECTOR => Some(UNISWAP_V3_DECODER_ID),
        EXACT_OUTPUT_SINGLE_SELECTOR => Some(EXACT_OUTPUT_SINGLE_DECODER_ID),
        EXACT_OUTPUT_SELECTOR => Some(EXACT_OUTPUT_DECODER_ID),

        // ── swap-router-02 ────────────────────────────────────────────────────
        SR02_EXACT_INPUT_SINGLE_SELECTOR => Some(SR02_EXACT_INPUT_SINGLE_DECODER_ID),
        SR02_EXACT_INPUT_SELECTOR => Some(SR02_EXACT_INPUT_DECODER_ID),
        SR02_EXACT_OUTPUT_SINGLE_SELECTOR => Some(SR02_EXACT_OUTPUT_SINGLE_DECODER_ID),
        SR02_EXACT_OUTPUT_SELECTOR => Some(SR02_EXACT_OUTPUT_DECODER_ID),

        // Universal Router is handled directly by `MultiRouterCallAdapter`
        // (which does opcode dispatch in `call-adapter`), so its selectors
        // intentionally don't appear here — fallback would never be reached
        // for UR addresses, and even if it were, there's no UR Mapper to
        // route to.

        // ── weth ──────────────────────────────────────────────────────────────
        WETH_DEPOSIT_SELECTOR => Some(WETH_DEPOSIT_DECODER_ID),
        WETH_WITHDRAW_SELECTOR => Some(WETH_WITHDRAW_DECODER_ID),

        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("address format: {0}")]
    AddressFormat(String),
    #[error("unsupported value: {0}")]
    UnsupportedValue(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_decoder_id_for_known_selectors() {
        assert_eq!(
            decoder_id_for_selector(SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR),
            Some(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID)
        );
        assert_eq!(
            decoder_id_for_selector(APPROVE_SELECTOR),
            Some(ERC20_APPROVE_DECODER_ID)
        );
    }

    #[test]
    fn test_decoder_id_for_unknown_selector_is_none() {
        assert_eq!(decoder_id_for_selector([0x00, 0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn test_fallback_decoder_id_format() {
        let id = fallback_decoder_id([0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(id.as_str(), "fallback/0xdeadbeef");
    }

    #[test]
    fn test_convert_value_primitives() {
        let v = convert_value(DynSolValue::Uint(U256::from(42u64), 256)).unwrap();
        assert!(matches!(v, DecodedValue::Uint(u) if u == U256::from(42u64)));

        let v = convert_value(DynSolValue::Bool(true)).unwrap();
        assert!(matches!(v, DecodedValue::Bool(true)));

        let v = convert_value(DynSolValue::String("hello".into())).unwrap();
        assert!(matches!(v, DecodedValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_convert_value_array() {
        let v = convert_value(DynSolValue::Array(vec![
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(2u64), 256),
        ]))
        .unwrap();
        let DecodedValue::Array(items) = v else {
            panic!("expected array");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_convert_legacy_call_uses_decoder_id_lookup() {
        let legacy = LegacyCall {
            function_name: "approve".into(),
            signature: "approve(address,uint256)".into(),
            args: vec![],
        };
        let converted = convert_legacy_call(legacy, APPROVE_SELECTOR).unwrap();
        assert_eq!(converted.decoder_id.as_str(), ERC20_APPROVE_DECODER_ID);
    }

    #[test]
    fn test_convert_legacy_call_unknown_uses_fallback_id() {
        let legacy = LegacyCall {
            function_name: "foo".into(),
            signature: "foo()".into(),
            args: vec![],
        };
        let converted = convert_legacy_call(legacy, [0xde, 0xad, 0xbe, 0xef]).unwrap();
        assert_eq!(converted.decoder_id.as_str(), "fallback/0xdeadbeef");
    }
}
