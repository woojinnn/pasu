//! Increase-liquidity enrichment placeholder.

use crate::action::dex::IncreaseLiquidityAction;
use crate::action::Address as ActionAddress;
use crate::host::HostCapabilities;

/// Enrich an increase-liquidity action when host facts become available.
pub(super) fn enrich(
    _action: &mut IncreaseLiquidityAction,
    _from: &ActionAddress,
    _target: &ActionAddress,
    _host: &HostCapabilities<'_>,
) {
}
