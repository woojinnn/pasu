//! 실제 publicnode 에 연결하는 통합 테스트.
//!
//! 네트워크가 없거나 publicnode 가 다운되면 실패하므로 기본은 `#[ignore]`.
//! 수동으로 돌리려면:
//! ```text
//! cargo test -p simulation-sync --test rpc_live -- --ignored
//! ```

use simulation_state::ChainId;
use simulation_sync::{BlockTag, RpcConfig, RpcRouter};

fn live_config() -> RpcConfig {
    let toml = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"

[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
    RpcConfig::load_str(toml).unwrap()
}

#[tokio::test]
#[ignore]
async fn live_block_number() {
    let router = RpcRouter::from_config(live_config()).unwrap();
    let n = router
        .eth_block_number(&ChainId::ethereum_mainnet())
        .await
        .expect("eth_blockNumber");
    println!("ethereum head = {}", n);
    assert!(n > 18_000_000, "block number suspiciously low: {}", n);
}

#[tokio::test]
#[ignore]
async fn live_gas_price() {
    let router = RpcRouter::from_config(live_config()).unwrap();
    let gas = router
        .eth_gas_price(&ChainId::ethereum_mainnet())
        .await
        .expect("eth_gasPrice");
    println!("gas price wei = {}", gas);
    // 1 gwei ~ 1e9. 100 gwei ~ 1e11. 정상이면 그 사이.
    assert!(gas > alloy_primitives::U256::from(100_000u64), "gas too low");
}

#[tokio::test]
#[ignore]
async fn live_usdc_total_supply_via_eth_call() {
    // USDC totalSupply() — function selector 0x18160ddd
    use simulation_sync::EthCallRequest;
    use std::str::FromStr;

    let router = RpcRouter::from_config(live_config()).unwrap();
    let usdc =
        alloy_primitives::Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();

    let req = EthCallRequest {
        to: usdc,
        data: vec![0x18, 0x16, 0x0d, 0xdd].into(),
        from: None,
        value: None,
        block: BlockTag::Latest,
    };
    let return_data = router
        .eth_call(&ChainId::ethereum_mainnet(), req)
        .await
        .expect("eth_call totalSupply");

    assert_eq!(return_data.len(), 32, "totalSupply 는 32-byte uint256");
    // 0 이 아닌 큰 수
    assert!(return_data.iter().any(|&b| b != 0));
}
