//! Burn-liquidity-NFT enrichment placeholder.

use crate::action::dex::BurnLiquidityNftAction;
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

/// Enrich a burn-liquidity-NFT action when host facts become available.
pub(super) const fn enrich(
    _action: &mut BurnLiquidityNftAction,
    _from: &ActionAddress,
    _target: &ActionAddress,
    _host: &HostCapabilities<'_>,
) {
}
