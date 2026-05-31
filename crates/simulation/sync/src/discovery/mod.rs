//! Wallet token discovery — figure out what tokens an address holds so
//! the orchestrator has something to refresh.
//!
//! EVM has no native "list tokens for an address" RPC. Three-tier
//! strategy:
//!   1. Native gas balance — always, via `eth_getBalance`. No key.
//!   2. Etherscan V2 indexer — when `ETHERSCAN_API_KEY` is set,
//!      lists every ERC-20 the address has ever held.
//!   3. Top-N hardcoded catalog (~30/chain) via Multicall `balanceOf` —
//!      fallback when no Etherscan key. Catches the canonical
//!      stablecoins + majors without an indexer.
//!
//! Output is a `Vec<DiscoveredToken>` the caller turns into
//! `TokenHolding` entries seeded into a `WalletState`. The orchestrator
//! then keeps prices fresh through normal `LiveField` refresh cycles.

pub mod coingecko;
pub mod etherscan;
pub mod native;
pub mod top_tokens;

pub use coingecko::CoinGeckoClient;
pub use etherscan::EtherscanClient;
pub use native::fetch_native_balance;
pub use top_tokens::discover_top_tokens;

use simulation_state::primitives::U256;
use simulation_state::token::TokenKey;

/// A single token found for a wallet. `balance` is the current on-chain
/// amount in the token's smallest unit (wei / token decimals).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredToken {
    pub key: TokenKey,
    pub symbol: String,
    pub decimals: u8,
    pub balance: U256,
}
