//! Shared helpers for UR-opcode mappers.

use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::U256;
use policy_engine::action::common::{
    AmountConstraint, AssetKind, AssetRef, AssetRefWithAmountConstraint, DecimalString,
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

pub(super) fn asset_with_amount(
    asset: AssetRef,
    amount: AmountConstraint,
) -> AssetRefWithAmountConstraint {
    AssetRefWithAmountConstraint { asset, amount }
}

pub(super) fn decimal_from_uint(value: U256) -> DecimalString {
    DecimalString::from_str(&value.to_string()).expect("U256 decimal string is always valid")
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
