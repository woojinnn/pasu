//! Shared helpers for UR-opcode mappers.

use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::U256;
use policy_engine::action::common::{
    AmountConstraint, AmountKind, AssetKind, AssetRef, AssetRefWithAmountConstraint, DecimalString,
};
use policy_engine::action::Address;

use crate::mapper::{MapContext, MapperError};

/// UR address sentinel — `0x...01` means "the original `msg.sender`" inside
/// UR opcode args. Translated to `ctx.from` by [`map_recipient`].
const ACTION_MSG_SENDER: &str = "0x0000000000000000000000000000000000000001";
/// UR address sentinel — `0x...02` means "this contract" (the router).
/// Translated to `ctx.to` by [`map_recipient`].
const ACTION_ADDRESS_THIS: &str = "0x0000000000000000000000000000000000000002";

/// Resolve a recipient address through the UR sentinel table.
pub(super) fn map_recipient(ctx: &MapContext<'_>, raw: Address) -> Address {
    let text = raw.to_string();
    if text == ACTION_MSG_SENDER {
        ctx.from.clone()
    } else if text == ACTION_ADDRESS_THIS {
        ctx.to.clone()
    } else {
        raw
    }
}

/// `AssetRef` for native ETH on the current chain.
pub(super) fn native_eth() -> AssetRef {
    AssetRef {
        kind: AssetKind::Native,
        address: None,
        token_id: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    }
}

/// `AssetRef` for WETH. We don't know WETH's per-chain address from inside
/// the mapper, so we surface it as an ERC20 without an address — downstream
/// (compactor / wallet UI) treats it as the canonical wrapped pair for the
/// chain. Future work: thread WETH addresses through `MapContext`.
pub(super) fn wrapped_weth() -> AssetRef {
    AssetRef {
        kind: AssetKind::Erc20,
        address: None,
        token_id: None,
        symbol: Some("WETH".to_owned()),
        decimals: Some(18),
    }
}

/// `AssetRef` for a token referenced by address inside a UR opcode (SWEEP,
/// TRANSFER, swap path entries). UR's `0x00…00` is the native-asset
/// sentinel; anything else is an ERC-20.
pub(super) fn token_asset_ref(addr: &Address) -> AssetRef {
    if is_zero_address(addr) {
        native_eth()
    } else {
        AssetRef {
            kind: AssetKind::Erc20,
            address: Some(addr.clone()),
            token_id: None,
            symbol: None,
            decimals: None,
        }
    }
}

fn is_zero_address(addr: &Address) -> bool {
    addr.to_string()
        .eq_ignore_ascii_case("0x0000000000000000000000000000000000000000")
}

pub(super) fn asset_with_amount(
    asset: AssetRef,
    amount: AmountConstraint,
) -> AssetRefWithAmountConstraint {
    AssetRefWithAmountConstraint { asset, amount }
}

pub(super) fn decimal_from_uint(value: U256) -> DecimalString {
    DecimalString::from_str(&value.to_string()).expect("U256 decimal string is always valid")
}

/// Convenience: build a `SwapAction.*_token.amount` constraint without
/// repeating `AmountConstraint { kind, value: Some(_) }` in every mapper.
pub(super) fn swap_amount_constraint(kind: AmountKind, value: DecimalString) -> AmountConstraint {
    AmountConstraint {
        kind,
        value: Some(value),
    }
}

/// Look up a [`DecodedArg`](abi_resolver::DecodedArg) by name and pull out
/// its `Address` value. UR opcodes always name their args at the outer
/// level, so name-based lookup is stable.
pub(super) fn find_address(decoded: &DecodedCall, name: &str) -> Result<Address, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            DecodedValue::Address(addr) => Some(addr.clone()),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

/// Look up a uint arg by name.
pub(super) fn find_uint(decoded: &DecodedCall, name: &str) -> Result<U256, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            DecodedValue::Uint(u) => Some(*u),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

/// Pull out a `bytes` arg by name. UR V3 swap opcodes embed the packed
/// V3 path as the `path` arg.
pub(super) fn find_bytes(decoded: &DecodedCall, name: &str) -> Result<Vec<u8>, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            DecodedValue::Bytes(b) => Some(b.clone()),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

/// Pull out an address array by name (V2 swap `path` arg).
pub(super) fn find_address_array(
    decoded: &DecodedCall,
    name: &str,
) -> Result<Vec<Address>, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            DecodedValue::Array(items) => Some(items),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))?
        .iter()
        .map(|v| match v {
            DecodedValue::Address(addr) => Ok(addr.clone()),
            _ => Err(MapperError::ArgumentMismatch {
                name: name.into(),
                message: "path entry must be address".into(),
            }),
        })
        .collect()
}

/// Result of parsing a V3 packed path: `addr(20) | fee(3) | addr(20) [| fee(3) | addr(20)]*`.
pub(super) struct ParsedV3Path {
    pub(super) token_in: Address,
    pub(super) token_out: Address,
    /// Fee of the *first* hop, expressed in basis points (hundredths of a
    /// percent). The V3 ABI encodes fees in hundredths of bps; we divide by
    /// 100 so downstream sees normal bps.
    pub(super) fee_bps: Option<u32>,
}

const V3_ADDRESS_LEN: usize = 20;
const V3_FEE_LEN: usize = 3;

/// Decode the packed V3 path that UR uses for `V3_SWAP_EXACT_IN/OUT`.
/// Mirrors the implementation in call-adapter's `multi_router::common`.
pub(super) fn parse_v3_path(path: &[u8]) -> Result<ParsedV3Path, MapperError> {
    let hop_len = V3_FEE_LEN + V3_ADDRESS_LEN;
    let min_len = V3_ADDRESS_LEN + hop_len;
    if path.len() < min_len || !(path.len() - V3_ADDRESS_LEN).is_multiple_of(hop_len) {
        return Err(MapperError::ArgumentMismatch {
            name: "path".into(),
            message: format!(
                "UR V3 path malformed: expected `addr(20) + (fee(3)+addr(20))+`, got {} bytes",
                path.len()
            ),
        });
    }
    let token_in = address_from_bytes(&path[..V3_ADDRESS_LEN])?;
    let token_out = address_from_bytes(&path[path.len() - V3_ADDRESS_LEN..])?;
    let first_fee = (u32::from(path[20]) << 16) | (u32::from(path[21]) << 8) | u32::from(path[22]);
    Ok(ParsedV3Path {
        token_in,
        token_out,
        fee_bps: Some(first_fee / 100),
    })
}

fn address_from_bytes(bytes: &[u8]) -> Result<Address, MapperError> {
    if bytes.len() != V3_ADDRESS_LEN {
        return Err(MapperError::ArgumentMismatch {
            name: "path".into(),
            message: format!("address slice must be {V3_ADDRESS_LEN} bytes"),
        });
    }
    let hex = format!("0x{}", hex::encode(bytes));
    Address::from_str(&hex).map_err(|err| MapperError::ArgumentMismatch {
        name: "path".into(),
        message: format!("invalid address bytes: {err}"),
    })
}

/// V2 path endpoints — `(first, last)` of an `address[]` swap path.
pub(super) fn path_endpoints(path: &[Address]) -> Result<(&Address, &Address), MapperError> {
    if path.len() < 2 {
        return Err(MapperError::ArgumentMismatch {
            name: "path".into(),
            message: "UR V2 path must contain at least 2 tokens".into(),
        });
    }
    Ok((&path[0], &path[path.len() - 1]))
}
