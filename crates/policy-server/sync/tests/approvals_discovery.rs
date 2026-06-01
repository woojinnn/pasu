use std::sync::Arc;

use simulation_state::{Address, ChainId};
use simulation_sync::discovery::discover_approvals;
use simulation_sync::fetchers::rpc::{RpcConfig, RpcRouter};

fn router() -> Arc<RpcRouter> {
    let toml_text = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"

[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
    Arc::new(RpcRouter::from_config(RpcConfig::load_str(toml_text).unwrap()).unwrap())
}

#[tokio::test]
async fn approval_discovery_returns_empty_for_uncataloged_chain_without_rpc() {
    let found = discover_approvals(
        &router(),
        &ChainId::new("eip155:999999"),
        Address::ZERO,
        &[Address::from([0x11; 20])],
    )
    .await
    .unwrap();

    assert!(found.is_empty());
}
