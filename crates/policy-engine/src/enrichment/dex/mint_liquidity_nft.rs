//! Mint-liquidity-NFT enrichment placeholder.

use crate::action::dex::MintLiquidityNftAction;
use crate::action::Address as ActionAddress;
use crate::enrichment::dispatch::Enrich;
use crate::host::HostCapabilities;

impl Enrich for MintLiquidityNftAction {
    fn enrich(
        &mut self,
        _from: &ActionAddress,
        _target: &ActionAddress,
        _host: &HostCapabilities<'_>,
    ) {
    }
}
