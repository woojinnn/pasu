//! Venue API 별 구현 — perp DEX / off-chain order venue.

pub mod hyperliquid;

pub use hyperliquid::HyperliquidFetcher;

// pub mod gmx;           // GM token, position state
// pub mod dydx;          // perpetual market + order indexer
// pub mod uniswap_x;     // off-chain order lifecycle
