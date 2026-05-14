//! Decrease-liquidity enrichment placeholder.

use crate::action::dex::DecreaseLiquidityAction;
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

/// Enrich a decrease-liquidity action when host facts become available.
pub(super) fn enrich(
    _action: &mut DecreaseLiquidityAction,
    _from: &ActionAddress,
    _target: &ActionAddress,
    _host: &HostCapabilities<'_>,
) {
}
