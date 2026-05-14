//! Burn-liquidity-NFT enrichment placeholder.

use crate::action::dex::BurnLiquidityNftAction;
use crate::action::Address as ActionAddress;
use crate::enrichment::dispatch::Enrich;
use crate::host::HostCapabilities;

impl Enrich for BurnLiquidityNftAction {
    fn enrich(
        &mut self,
        _from: &ActionAddress,
        _target: &ActionAddress,
        _host: &HostCapabilities<'_>,
    ) {
    }
}
