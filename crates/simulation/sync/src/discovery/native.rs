//! Native gas-token balance via plain `eth_getBalance` — works without
//! any indexer / API key. Used as the always-on baseline for wallet
//! discovery so even an unconfigured server can show ETH/MATIC/etc.

use std::sync::Arc;

use simulation_state::primitives::{Address, ChainId, U256};
use simulation_state::token::TokenKey;

use crate::error::SyncError;
use crate::fetchers::rpc::{BlockTag, RpcRouter};

use super::DiscoveredToken;

/// Fetch the native balance for `address` on `chain`. Returns a
/// `DiscoveredToken` with `TokenKey::Native` (symbol/decimals are the
/// canonical EVM "ETH"/18 for now; per-chain overrides land when
/// non-ETH gas tokens are supported).
pub async fn fetch_native_balance(
    router: &Arc<RpcRouter>,
    chain: &ChainId,
    address: Address,
) -> Result<DiscoveredToken, SyncError> {
    let balance: U256 = router.eth_balance(chain, address, BlockTag::Latest).await?;
    Ok(DiscoveredToken {
        key: TokenKey::Native {
            chain: chain.clone(),
        },
        symbol: native_symbol(chain).to_string(),
        decimals: 18,
        balance,
    })
}

/// Friendly symbol per chain. Falls back to "GAS" for unknown chains.
fn native_symbol(chain: &ChainId) -> &'static str {
    match chain.as_str() {
        "eip155:1" | "eip155:42161" | "eip155:8453" | "eip155:10" => "ETH",
        "eip155:137" => "MATIC",
        "eip155:56" => "BNB",
        "eip155:43114" => "AVAX",
        _ => "GAS",
    }
}
