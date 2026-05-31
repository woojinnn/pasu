//! Wallet token discovery — figure out what tokens an address holds so
//! the orchestrator has something to refresh.
//!
//! EVM has no native "list tokens for an address" RPC; we lean on an
//! indexer (Etherscan V2 if a key is configured) and on `eth_getBalance`
//! for the native gas token (always available, no key needed).
//!
//! The output is a `Vec<DiscoveredToken>` the caller turns into
//! `TokenHolding` entries seeded into a `WalletState`. The orchestrator
//! then keeps prices fresh through normal `LiveField` refresh cycles.

pub mod etherscan;
pub mod native;

pub use etherscan::EtherscanClient;
pub use native::fetch_native_balance;

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
