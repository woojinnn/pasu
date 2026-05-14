//! Per-action enrichment handlers.

mod add_liquidity;
mod burn_liquidity_nft;
mod decrease_liquidity;
mod increase_liquidity;
mod mint_liquidity_nft;
mod remove_liquidity;
mod swap;

use crate::action::dex::{
    AddLiquidityAction, BurnLiquidityNftAction, DecreaseLiquidityAction, IncreaseLiquidityAction,
    MintLiquidityNftAction, RemoveLiquidityAction, SwapAction,
};
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

pub(super) fn enrich_swap(
    action: &mut SwapAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    swap::enrich(action, from, target, host);
}

pub(super) fn enrich_add_liquidity(
    action: &mut AddLiquidityAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    add_liquidity::enrich(action, from, target, host);
}

pub(super) fn enrich_remove_liquidity(
    action: &mut RemoveLiquidityAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    remove_liquidity::enrich(action, from, target, host);
}

pub(super) fn enrich_mint_liquidity_nft(
    action: &mut MintLiquidityNftAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    mint_liquidity_nft::enrich(action, from, target, host);
}

pub(super) fn enrich_burn_liquidity_nft(
    action: &mut BurnLiquidityNftAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    burn_liquidity_nft::enrich(action, from, target, host);
}

pub(super) fn enrich_increase_liquidity(
    action: &mut IncreaseLiquidityAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    increase_liquidity::enrich(action, from, target, host);
}

pub(super) fn enrich_decrease_liquidity(
    action: &mut DecreaseLiquidityAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    decrease_liquidity::enrich(action, from, target, host);
}
