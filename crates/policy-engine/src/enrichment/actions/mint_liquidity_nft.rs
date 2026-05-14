//! Mint-liquidity-NFT enrichment placeholder.

use crate::action::dex::MintLiquidityNftAction;
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

/// Enrich a mint-liquidity-NFT action when host facts become available.
pub(super) const fn enrich(
    _action: &mut MintLiquidityNftAction,
    _from: &ActionAddress,
    _target: &ActionAddress,
    _host: &HostCapabilities<'_>,
) {
}
