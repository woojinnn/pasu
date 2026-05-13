//! Shared helpers for V3 SwapRouter / SwapRouter02 mappers.

use alloy_primitives::Address as AlloyAddress;

use crate::context::BuildContext;
use crate::error::MapError;
use crate::types::actions::SwapAction;
use crate::types::envelope::{ActionEnvelope, ActionFields, Category};

pub const V3_ROUTER_MAINNET_LC: &str = "0xe592427a0aece92de3edee1f18e0157c05861564";
pub const V3_ROUTER02_MAINNET_LC: &str = "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45";

/// V3 path = `token (20) || fee (3) || token (20) || fee (3) || ... || token (20)`.
/// Returns the list of token addresses and the list of `fee` tier values
/// (length one less than tokens).
pub fn parse_path(path: &[u8]) -> Result<(Vec<AlloyAddress>, Vec<u32>), MapError> {
    const ADDR: usize = 20;
    const FEE: usize = 3;
    const HOP: usize = ADDR + FEE;
    if path.len() < ADDR + HOP {
        return Err(MapError::EmptyPath(path.len()));
    }
    if !(path.len() - ADDR).is_multiple_of(HOP) {
        return Err(MapError::AbiDecode(format!(
            "V3 path length {} not aligned to address(20) + N*hop(23)",
            path.len()
        )));
    }
    let n_hops = (path.len() - ADDR) / HOP;
    let mut tokens = Vec::with_capacity(n_hops + 1);
    let mut fees = Vec::with_capacity(n_hops);
    tokens.push(AlloyAddress::from_slice(&path[0..ADDR]));
    let mut off = ADDR;
    for _ in 0..n_hops {
        let fee = u32::from_be_bytes([0, path[off], path[off + 1], path[off + 2]]);
        fees.push(fee);
        off += FEE;
        tokens.push(AlloyAddress::from_slice(&path[off..off + ADDR]));
        off += ADDR;
    }
    Ok((tokens, fees))
}

/// Convert a V3 fee tier (uint24 ppm) to bps. 3000 ppm → 30 bps.
pub fn fee_tier_to_bps(fee_tier: u32) -> u32 {
    fee_tier / 100
}

pub fn deadline_from(deadline_u256: alloy_primitives::U256, ctx: &BuildContext) -> Option<i64> {
    let d: i64 = deadline_u256.try_into().unwrap_or(i64::MAX);
    Some(d - ctx.block_timestamp)
}

pub fn envelope_swap(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope::new(Category::Dex, ActionFields::Swap(action))
}
