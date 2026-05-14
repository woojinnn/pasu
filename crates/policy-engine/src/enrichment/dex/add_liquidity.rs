//! Add-liquidity enrichment placeholder.

use crate::action::dex::AddLiquidityAction;
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

/// Enrich an add-liquidity action when host facts become available.
pub(super) const fn enrich(
    _action: &mut AddLiquidityAction,
    _from: &ActionAddress,
    _target: &ActionAddress,
    _host: &HostCapabilities<'_>,
) {
}
