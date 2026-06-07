use async_trait::async_trait;

use policy_state::pending::PendingTx;
use policy_state::primitives::{Address, Time};

use crate::error::SyncError;

pub mod hyperliquid;
pub mod ttl_cache;
pub mod uniswap;

pub use hyperliquid::HyperliquidFetcher;
pub use uniswap::UniswapFetcher;

pub mod cow_swap;
pub mod one_inch_fusion;
pub mod one_inch_fusion_plus;
pub mod one_inch_lop;
pub mod uniswap_x;

pub use cow_swap::CowSwapFetcher;
pub use one_inch_fusion::OneInchFusionFetcher;
pub use one_inch_fusion_plus::OneInchFusionPlusFetcher;
pub use one_inch_lop::OneInchLopFetcher;
pub use uniswap_x::{UniswapXFetcher, UniswapXOrder};

// pub mod gmx;           // GM token, position state
// pub mod dydx;          // perpetual market + order indexer

/// Off-chain intent-order discovery for a single venue.
///
/// Each implementor owns its own request loop, response parsing, and projection
/// into the canonical `PendingTx` shape, so the orchestrator can dispatch over a
/// heterogeneous set of venues (`UniswapX`, `CowSwap`, `1inch Fusion`, …)
/// uniformly. `fetch_orders` returns the swapper's currently-discoverable orders
/// projected into `PendingTx`. Terminal-order pruning and upsert-by-id are
/// handled by the orchestrator's `upsert_intent_orders`, not the fetcher.
#[async_trait]
pub trait IntentFetcher: Send + Sync {
    async fn fetch_orders(&self, swapper: &Address, now: Time)
        -> Result<Vec<PendingTx>, SyncError>;

    /// When `Some(prefix)`, this fetcher's `fetch_orders` return value is the
    /// **complete** current set of `PendingTx` ids under `prefix`. This is for
    /// "active-orderbook" venues (e.g. 1inch LOP, Fusion+) where a terminal
    /// order silently drops off the listing instead of appearing with a terminal
    /// status — so `upsert_intent_orders` alone would leave a stale `Active`
    /// entry tracked forever. `sync_intent_orders` snapshot-prunes any tracked id
    /// under `prefix` absent from the returned set, **but only on a successful
    /// (`Ok`) fetch**.
    ///
    /// CONTRACT: a fetcher returning `Some(_)` MUST return `Err` on any
    /// incompleteness (a failed chain in a multi-chain loop, a mid-walk
    /// pagination error) rather than a partial `Ok` — otherwise live orders on
    /// the un-fetched portion would be wrongly pruned. The default `None` opts
    /// out (venues whose API returns terminal orders explicitly, pruned by
    /// `upsert_intent_orders`).
    fn authoritative_prefix(&self) -> Option<&str> {
        None
    }
}
