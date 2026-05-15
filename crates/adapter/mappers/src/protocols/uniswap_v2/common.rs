//! Shared helpers for Uniswap V2 router mappers.

use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::U256;
use policy_engine::action::dex::SwapAction;
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};

use crate::mapper::{MapContext, MapperError};

pub(super) fn swap_envelope(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    }
}

pub(super) fn path_assets(
    ctx: &MapContext<'_>,
    path: &[Address],
) -> Result<(AssetRef, AssetRef), MapperError> {
    if path.len() < 2 {
        return Err(MapperError::ArgumentMismatch {
            name: "path".to_owned(),
            message: format!("expected at least two token addresses, got {}", path.len()),
        });
    }
    let token_in = path.first().expect("path length checked");
    let token_out = path.last().expect("path length checked");

    Ok((asset_ref(ctx, token_in), asset_ref(ctx, token_out)))
}

pub(super) fn path_first_asset(
    ctx: &MapContext<'_>,
    path: &[Address],
) -> Result<AssetRef, MapperError> {
    if path.len() < 2 {
        return Err(MapperError::ArgumentMismatch {
            name: "path".to_owned(),
            message: format!("expected at least two token addresses, got {}", path.len()),
        });
    }

    Ok(asset_ref(
        ctx,
        path.first().expect("path length checked by len"),
    ))
}

pub(super) fn path_last_asset(
    ctx: &MapContext<'_>,
    path: &[Address],
) -> Result<AssetRef, MapperError> {
    if path.len() < 2 {
        return Err(MapperError::ArgumentMismatch {
            name: "path".to_owned(),
            message: format!("expected at least two token addresses, got {}", path.len()),
        });
    }

    Ok(asset_ref(
        ctx,
        path.last().expect("path length checked by len"),
    ))
}

pub(super) fn asset_ref(ctx: &MapContext<'_>, address: &Address) -> AssetRef {
    let metadata = ctx.token_registry.lookup(ctx.chain_id, address);
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id: ctx.chain_id,
        address: Some(address.clone()),
        symbol: metadata.as_ref().map(|m| m.symbol.clone()),
        decimals: metadata.map(|m| m.decimals),
    }
}

pub(super) fn native_eth_asset_ref(ctx: &MapContext<'_>) -> AssetRef {
    AssetRef {
        kind: AssetKind::Native,
        chain_id: ctx.chain_id,
        address: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    }
}

pub(super) fn amount_constraint(
    kind: AmountKind,
    value: U256,
) -> Result<AmountConstraint, MapperError> {
    Ok(AmountConstraint {
        kind,
        value: Some(decimal(value)?),
    })
}

pub(super) fn decimal_amount_constraint(
    kind: AmountKind,
    value: &DecimalString,
) -> AmountConstraint {
    AmountConstraint {
        kind,
        value: Some(value.clone()),
    }
}

pub(super) fn validity(deadline: U256) -> Result<Validity, MapperError> {
    Ok(Validity {
        expires_at: decimal(deadline)?,
        source: ValiditySource::TxDeadline,
    })
}

pub(super) fn decimal(value: U256) -> Result<DecimalString, MapperError> {
    DecimalString::from_str(&value.to_string())
        .map_err(|e| MapperError::Internal(anyhow::anyhow!(e)))
}

pub(super) fn uint_arg(decoded: &DecodedCall, name: &str) -> Result<U256, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Uint(value) => Ok(*value),
        other => Err(argument_mismatch(
            name,
            format!("expected uint256, got {other:?}"),
        )),
    }
}

pub(super) fn address_arg(decoded: &DecodedCall, name: &str) -> Result<Address, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Address(value) => Ok(value.clone()),
        other => Err(argument_mismatch(
            name,
            format!("expected address, got {other:?}"),
        )),
    }
}

pub(super) fn address_array_arg(
    decoded: &DecodedCall,
    name: &str,
) -> Result<Vec<Address>, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Array(values) => values
            .iter()
            .map(|value| match value {
                DecodedValue::Address(address) => Ok(address.clone()),
                other => Err(argument_mismatch(
                    name,
                    format!("expected address[] item, got {other:?}"),
                )),
            })
            .collect(),
        other => Err(argument_mismatch(
            name,
            format!("expected address[], got {other:?}"),
        )),
    }
}

fn arg<'a>(decoded: &'a DecodedCall, name: &str) -> Result<&'a DecodedValue, MapperError> {
    decoded
        .args
        .iter()
        .find(|arg| arg.name == name)
        .map(|arg| &arg.value)
        .ok_or_else(|| MapperError::MissingArgument(name.to_owned()))
}

pub(super) fn argument_mismatch(name: &str, message: String) -> MapperError {
    MapperError::ArgumentMismatch {
        name: name.to_owned(),
        message,
    }
}
