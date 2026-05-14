//! Remove-liquidity enrichment placeholder.

use crate::action::dex::RemoveLiquidityAction;
use crate::action::Address as ActionAddress;
use crate::enrichment::dispatch::Enrich;
use crate::host::HostCapabilities;

impl Enrich for RemoveLiquidityAction {
    fn enrich(
        &mut self,
        _from: &ActionAddress,
        _target: &ActionAddress,
        _host: &HostCapabilities<'_>,
    ) {
    }
}
