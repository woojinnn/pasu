//! Shared helpers for V2 Router02 mappers.

use alloy_primitives::Address as AlloyAddress;

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::SwapAction;
use crate::types::common::AssetRef;
use crate::types::envelope::{ActionEnvelope, ActionFields, Category};

pub const V2_ROUTER_MAINNET_LC: &str = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d";
pub const V2_FEE_BPS: u32 = 30;

#[derive(Clone, Copy)]
pub struct EthSide {
    pub input_is_native: bool,
    pub output_is_native: bool,
}

pub const TT: EthSide = EthSide {
    input_is_native: false,
    output_is_native: false,
};
pub const ET: EthSide = EthSide {
    input_is_native: true,
    output_is_native: false,
};
pub const TE: EthSide = EthSide {
    input_is_native: false,
    output_is_native: true,
};

pub fn token_in_out(
    ctx: &BuildContext,
    path: &[AlloyAddress],
    eth: EthSide,
) -> Result<(AssetRef, AssetRef), MapError> {
    if path.len() < 2 {
        return Err(MapError::EmptyPath(path.len()));
    }
    let a = *path.first().unwrap();
    let b = *path.last().unwrap();
    let token_in = if eth.input_is_native {
        ctx.tokens.native(ctx.chain_id)
    } else {
        ctx.tokens.erc20(ctx.chain_id, a)
    };
    let token_out = if eth.output_is_native {
        ctx.tokens.native(ctx.chain_id)
    } else {
        ctx.tokens.erc20(ctx.chain_id, b)
    };
    Ok((token_in, token_out))
}

pub fn deadline_from(deadline_u256: alloy_primitives::U256, ctx: &BuildContext) -> Option<i64> {
    let d: i64 = deadline_u256.try_into().unwrap_or(i64::MAX);
    Some(d - ctx.block_timestamp)
}

pub fn envelope_swap(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope::new(Category::Dex, ActionFields::Swap(action))
}

/// Pull `tx.value` as a `U256`, used by payable V2 functions where the
/// input amount comes from `msg.value` rather than calldata.
pub fn tx_value_u256(tx: &RawTx) -> alloy_primitives::U256 {
    tx.value
        .parse::<alloy_primitives::U256>()
        .unwrap_or_default()
}
