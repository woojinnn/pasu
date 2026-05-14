//! Remove-liquidity enrichment placeholder.

use crate::action::dex::RemoveLiquidityAction;
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

/// Enrich a remove-liquidity action when host facts become available.
pub(super) const fn enrich(
    _action: &mut RemoveLiquidityAction,
    _from: &ActionAddress,
    _target: &ActionAddress,
    _host: &HostCapabilities<'_>,
) {
}
