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

#[tokio::test]
#[ignore]
async fn live_sync_primitives_block_height_and_balances() {
    // 실제 wallet (Vitalik) 의 ETH + USDC 잔고를 sync_primitives 로 갱신.
    use simulation_state::{
        Address, Balance, BaseCategory, DataSource, FiatCurrency, PegTarget, Time, TokenHolding,
        TokenKey, TokenKind, WalletId, WalletState,
    };
    use simulation_sync::Orchestrator;
    use std::str::FromStr;
    use std::sync::Arc;

    let router = Arc::new(RpcRouter::from_config(live_config()).unwrap());
    let orch = Orchestrator::from_rpc_router(router);

    let vitalik =
        Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap();
    let mut state = WalletState::new(WalletId::new(vitalik, [ChainId::ethereum_mainnet()]));

    // Native (ETH) holding placeholder
    let native_key = TokenKey::Native {
        chain: ChainId::ethereum_mainnet(),
    };
    state.tokens.insert(
        native_key.clone(),
        TokenHolding {
            key: native_key.clone(),
            kind: TokenKind::NativeGas,
            symbol: "ETH".into(),
            decimals: 18,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            last_synced_at: Time::from_unix(0),
            primitives_source: DataSource::UserSupplied,
        },
    );

    // USDC holding placeholder
    let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
    let usdc_key = TokenKey::Erc20 {
        chain: ChainId::ethereum_mainnet(),
        address: usdc,
    };
    state.tokens.insert(
        usdc_key.clone(),
        TokenHolding {
            key: usdc_key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            last_synced_at: Time::from_unix(0),
            primitives_source: DataSource::UserSupplied,
        },
    );

    let report = orch
        .sync_primitives(&mut state, Time::from_unix(1_738_000_000))
        .await
        .unwrap();

    println!("primitives report: {:?}", report);
    assert_eq!(report.block_heights_updated, 1);
    assert!(state.block_heights.contains_key(&ChainId::ethereum_mainnet()));
    assert_eq!(report.native_balances_updated, 1);
    assert_eq!(report.erc20_balances_updated, 1);

    // Vitalik 은 ETH 보유 — 0 이상
    let eth_bal = state.tokens[&native_key].balance.as_fungible().unwrap();
    println!("ETH balance (wei) = {}", eth_bal);
}
