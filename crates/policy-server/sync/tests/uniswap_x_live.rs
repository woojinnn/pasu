//! Live Uniswap Trade API smoke test. Skipped unless `UNISWAP_KEY` is set, so
//! CI stays offline by default (mirrors `rpc_live.rs`).

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
        orders_endpoint: "https://trade-api.gateway.uniswap.org/v1".into(),
        api_key: key,
        chains: vec![ChainId::ethereum_mainnet()],
    });
    // A well-known address; we only assert the call succeeds and decodes.
    let swapper = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();
    let orders = fetcher
        .fetch_orders(&swapper, &ChainId::ethereum_mainnet())
        .await
        .expect("fetch ok");
    println!("fetched {} orders", orders.len());
}
