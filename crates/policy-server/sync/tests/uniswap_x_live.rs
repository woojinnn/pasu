//! Live Uniswap v2 order-service smoke test. Skipped unless `UNISWAP_KEY` is
//! set, so CI stays offline by default (mirrors `rpc_live.rs`). The v2 service
//! is public, so the key is not actually required — the gate just keeps the
//! network call opt-in.

use std::str::FromStr;

use policy_state::primitives::{Address, ChainId};
use policy_sync::config::UniswapConfig;
use policy_sync::fetchers::UniswapXFetcher;

#[tokio::test]
async fn fetch_orders_live_smoke() {
    let Ok(key) = std::env::var("UNISWAP_KEY") else {
        eprintln!("UNISWAP_KEY not set — skipping live test");
        return;
    };
    let fetcher = UniswapXFetcher::from_sync_config(&UniswapConfig {
        orders_endpoint: "https://api.uniswap.org/v2".into(),
        api_key: key,
        chains: vec![ChainId::ethereum_mainnet()],
    });
    // A public address with UniswapX history exercises the real decode path.
    let swapper = Address::from_str("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap();
    let orders = fetcher.fetch_orders(&swapper).await.expect("fetch ok");
    println!("fetched {} orders", orders.len());
    for o in orders.iter().take(3) {
        println!(
            "  {} chain={} status={} type={}",
            o.order_hash, o.chain_id, o.order_status, o.order_type
        );
    }
}
