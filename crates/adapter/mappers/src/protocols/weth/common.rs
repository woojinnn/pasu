//! Shared helpers for WETH mappers.

use abi_resolver::{DecodedCall, DecodedValue};
use policy_engine::action::common::{AssetKind, AssetRef};

use crate::mapper::{MapContext, MapperError};

pub(super) fn native_eth(chain_id: u64) -> AssetRef {
    AssetRef {
        kind: AssetKind::Native,
        chain_id,
        address: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    }
}

pub(super) fn wrapped_weth(ctx: &MapContext<'_>) -> AssetRef {
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id: ctx.chain_id,
        address: Some(ctx.to.clone()),
        symbol: Some("WETH".to_owned()),
        decimals: Some(18),
    }
}

pub(super) fn find_uint(
    decoded: &DecodedCall,
    name: &str,
) -> Result<alloy_primitives::U256, MapperError> {
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
