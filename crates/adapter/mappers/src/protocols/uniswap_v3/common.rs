//! Shared helpers for Uniswap V3 mappers.

use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::U256;
use policy_engine::action::dex::SwapAction;
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};

use crate::mapper::{MapContext, MapperError};

pub(super) const EXACT_INPUT_SINGLE_SIGNATURE: &str =
    "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))";
pub(super) const EXACT_INPUT_SIGNATURE: &str =
    "exactInput((bytes,address,uint256,uint256,uint256))";
pub(super) const EXACT_OUTPUT_SINGLE_SIGNATURE: &str =
    "exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))";
pub(super) const EXACT_OUTPUT_SIGNATURE: &str =
    "exactOutput((bytes,address,uint256,uint256,uint256))";

pub(super) fn swap_envelope(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    }
}

pub(super) struct ParsedPath {
    pub(super) token_in: Address,
    pub(super) token_out: Address,
    pub(super) first_fee: u32,
}

pub(super) fn parse_path(path: &[u8]) -> Result<ParsedPath, MapperError> {
    const ADDRESS_LEN: usize = 20;
    const FEE_LEN: usize = 3;
    const HOP_LEN: usize = ADDRESS_LEN + FEE_LEN;

    if path.len() < ADDRESS_LEN + HOP_LEN {
        return Err(argument_mismatch(
            "path",
            format!(
                "expected at least one V3 hop with 43 bytes, got {}",
                path.len()
            ),
        ));
    }
    if !(path.len() - ADDRESS_LEN).is_multiple_of(HOP_LEN) {
        return Err(argument_mismatch(
            "path",
            format!(
                "expected length 20 + 23*N for Uniswap V3 packed path, got {}",
                path.len()
            ),
        ));
    }

    let first_fee = (u32::from(path[20]) << 16) | (u32::from(path[21]) << 8) | u32::from(path[22]);
    let token_in = address_from_bytes(&path[..ADDRESS_LEN])?;
    let token_out = address_from_bytes(&path[path.len() - ADDRESS_LEN..])?;

    Ok(ParsedPath {
        token_in,
        token_out,
        first_fee,
    })
}

fn address_from_bytes(bytes: &[u8]) -> Result<Address, MapperError> {
    Address::from_str(&format!("0x{}", hex::encode(bytes)))
        .map_err(|e| MapperError::Internal(anyhow::anyhow!(e)))
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

pub(super) fn amount_constraint(
    kind: AmountKind,
    value: U256,
) -> Result<AmountConstraint, MapperError> {
    Ok(AmountConstraint {
        kind,
        value: Some(decimal(value)?),
    })
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

pub(super) fn fee_bps(value: U256) -> Result<u32, MapperError> {
    let fee: u32 = value
        .try_into()
        .map_err(|e| MapperError::Internal(anyhow::anyhow!("fee value out of range: {e}")))?;
    Ok(fee / 100)
}

pub(super) fn uint_arg(decoded: &DecodedCall, name: &str) -> Result<U256, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Uint(value) => Ok(*value),
        other => Err(argument_mismatch(
            name,
            format!("expected uint, got {other:?}"),
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

pub(super) fn bytes_arg<'a>(
    decoded: &'a DecodedCall,
    name: &str,
) -> Result<&'a [u8], MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Bytes(value) => Ok(value),
        other => Err(argument_mismatch(
            name,
            format!("expected bytes, got {other:?}"),
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

// Per-function map_* helpers are co-located here so the umbrella `UniswapV3Mapper`
// (which dispatches on function_signature) can reuse the exact same logic that
// the per-function `*ExactOutputSingleMapper` / `*ExactOutputMapper` use.

pub(super) fn map_exact_input_single(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    use policy_engine::action::dex::{SwapEnrichment, SwapMode};

    let token_in = address_arg(decoded, "tokenIn")?;
    let token_out = address_arg(decoded, "tokenOut")?;
    let fee = uint_arg(decoded, "fee")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_in = uint_arg(decoded, "amountIn")?;
    let amount_out_minimum = uint_arg(decoded, "amountOutMinimum")?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, &token_in),
        token_out: asset_ref(ctx, &token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
        amount_out: amount_constraint(AmountKind::Min, amount_out_minimum)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(fee_bps(fee)?),
        enrichment: SwapEnrichment::default(),
    }))
}

pub(super) fn map_exact_input(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    use policy_engine::action::dex::{SwapEnrichment, SwapMode};

    let path = bytes_arg(decoded, "path")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_in = uint_arg(decoded, "amountIn")?;
    let amount_out_minimum = uint_arg(decoded, "amountOutMinimum")?;
    let parsed = parse_path(path)?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, &parsed.token_in),
        token_out: asset_ref(ctx, &parsed.token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
        amount_out: amount_constraint(AmountKind::Min, amount_out_minimum)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(parsed.first_fee / 100),
        enrichment: SwapEnrichment::default(),
    }))
}

pub(super) fn map_exact_output_single(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    use policy_engine::action::dex::{SwapEnrichment, SwapMode};

    let token_in = address_arg(decoded, "tokenIn")?;
    let token_out = address_arg(decoded, "tokenOut")?;
    let fee = uint_arg(decoded, "fee")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_out = uint_arg(decoded, "amountOut")?;
    let amount_in_maximum = uint_arg(decoded, "amountInMaximum")?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: asset_ref(ctx, &token_in),
        token_out: asset_ref(ctx, &token_out),
        amount_in: amount_constraint(AmountKind::Max, amount_in_maximum)?,
        amount_out: amount_constraint(AmountKind::Exact, amount_out)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(fee_bps(fee)?),
        enrichment: SwapEnrichment::default(),
    }))
}

pub(super) fn map_exact_output(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    use policy_engine::action::dex::{SwapEnrichment, SwapMode};

    let path = bytes_arg(decoded, "path")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_out = uint_arg(decoded, "amountOut")?;
    let amount_in_maximum = uint_arg(decoded, "amountInMaximum")?;
    let parsed = parse_path(path)?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: asset_ref(ctx, &parsed.token_out),
        token_out: asset_ref(ctx, &parsed.token_in),
        amount_in: amount_constraint(AmountKind::Max, amount_in_maximum)?,
        amount_out: amount_constraint(AmountKind::Exact, amount_out)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(parsed.first_fee / 100),
        enrichment: SwapEnrichment::default(),
    }))
}
