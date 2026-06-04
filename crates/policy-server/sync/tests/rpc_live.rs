//! ```text
//! cargo test -p policy-sync --test rpc_live -- --ignored
//! ```

use policy_state::ChainId;
use policy_sync::{BlockTag, RpcConfig, RpcRouter, SyncConfig};

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

fn live_sync_config() -> SyncConfig {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_dir)
        .join("..")
        .join("..")
        .join("..")
        .join("pasu-sync.toml");
    SyncConfig::load_file(&path).unwrap_or_else(|e| panic!("load_file({}): {}", path.display(), e))
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
    assert!(
        gas > alloy_primitives::U256::from(100_000u64),
        "gas too low"
    );
}

#[tokio::test]
#[ignore]
async fn live_usdc_total_supply_via_eth_call() {
    // USDC totalSupply() — function selector 0x18160ddd
    use policy_sync::EthCallRequest;
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

    assert_eq!(
        return_data.len(),
        32,
        "totalSupply returns a 32-byte uint256"
    );
    assert!(return_data.iter().any(|&b| b != 0));
}

#[tokio::test]
#[ignore]
async fn live_sync_primitives_block_height_and_balances() {
    use policy_state::{
        Address, Balance, BaseCategory, DataSource, FiatCurrency, PegTarget, Time, TokenHolding,
        TokenKey, TokenKind, WalletId, WalletState,
    };
    use policy_sync::Orchestrator;
    use std::str::FromStr;
    use std::sync::Arc;

    let router = Arc::new(RpcRouter::from_config(live_config()).unwrap());
    let orch = Orchestrator::from_rpc_router(router);

    let vitalik = Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap();
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
            metadata: None,
            value_usd: None,
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
            metadata: None,
            value_usd: None,
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
    assert!(state
        .block_heights
        .contains_key(&ChainId::ethereum_mainnet()));
    assert_eq!(report.native_balances_updated, 1);
    assert_eq!(report.erc20_balances_updated, 1);

    let eth_bal = state.tokens[&native_key].balance.as_fungible().unwrap();
    println!("ETH balance (wei) = {}", eth_bal);
}

#[tokio::test]
#[ignore]
async fn live_chainlink_real_prices() {
    use policy_state::{DataSource, OracleProvider};
    use policy_sync::fetchers::ChainlinkFetcher;
    use std::sync::Arc;

    let sync_cfg = live_sync_config();
    let router = Arc::new(RpcRouter::from_config(sync_cfg.rpc.clone()).unwrap());
    let fetcher = ChainlinkFetcher::from_sync_config(router, &sync_cfg.oracles.chainlink);

    for feed in ["USDC/USD", "ETH/USD", "WBTC/USD"] {
        let source = DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: feed.into(),
        };
        let price = fetcher.fetch_price(&source).await.expect(feed);
        println!("{} = {}", feed, price.as_str());

        assert!(price.as_str() != "0", "{} returned zero", feed);
    }
}

#[tokio::test]
#[ignore]
async fn live_coingecko_real_prices() {
    use policy_state::{DataSource, OracleProvider};
    use policy_sync::{PriceFetcher, RestJsonOracleFetcher};

    let sync_cfg = live_sync_config();
    let rest = sync_cfg
        .oracles
        .rest
        .get("coingecko")
        .expect("[oracles.rest.coingecko] missing from toml");
    let fetcher = RestJsonOracleFetcher::from_sync_config("coingecko", rest);

    for feed in ["USDC/USD", "ETH/USD", "WBTC/USD"] {
        let source = DataSource::OracleFeed {
            provider: OracleProvider::Other("coingecko".into()),
            feed_id: feed.into(),
        };
        let price = fetcher.fetch_price(&source).await.expect(feed);
        println!("[coingecko] {} = {}", feed, price.as_str());
        assert!(price.as_str() != "0", "{} returned zero", feed);
    }
}

#[tokio::test]
#[ignore]
async fn live_orchestrator_routes_oracle_by_provider() {
    use policy_state::{
        Address, Balance, BaseCategory, DataSource, Decimal, Duration as SDuration, FiatCurrency,
        LiveField, OracleProvider, PegTarget, Time, TokenHolding, TokenKey, TokenKind, WalletId,
        WalletState,
    };
    use policy_sync::Orchestrator;
    use std::str::FromStr;

    let orch = Orchestrator::from_sync_config(&live_sync_config()).unwrap();

    let usdc_addr = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
    let mk_key = || TokenKey::Erc20 {
        chain: ChainId::ethereum_mainnet(),
        address: usdc_addr,
    };

    let mut state_chainlink =
        WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
    state_chainlink.tokens.insert(
        mk_key(),
        TokenHolding {
            key: mk_key(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(
                LiveField::new(
                    Decimal::new("0"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(1),
                )
                .with_ttl(SDuration::from_secs(1)),
            ),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1),
            primitives_source: DataSource::UserSupplied,
        },
    );

    let mut state_coingecko =
        WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
    state_coingecko.tokens.insert(
        mk_key(),
        TokenHolding {
            key: mk_key(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(
                LiveField::new(
                    Decimal::new("0"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Other("coingecko".into()),
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(1),
                )
                .with_ttl(SDuration::from_secs(1)),
            ),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1),
            primitives_source: DataSource::UserSupplied,
        },
    );

    let now = Time::from_unix(1_738_000_000);
    let r1 = orch.refresh(&mut state_chainlink, now).await.unwrap();
    let r2 = orch.refresh(&mut state_coingecko, now).await.unwrap();

    let p_chainlink = state_chainlink.tokens[&mk_key()]
        .price_usd
        .as_ref()
        .unwrap()
        .value
        .as_str()
        .to_string();
    let p_coingecko = state_coingecko.tokens[&mk_key()]
        .price_usd
        .as_ref()
        .unwrap()
        .value
        .as_str()
        .to_string();

    println!("chainlink USDC/USD = {} (report {:?})", p_chainlink, r1);
    println!("coingecko USDC/USD = {} (report {:?})", p_coingecko, r2);

    assert_ne!(p_chainlink, "0", "chainlink not refreshed");
    assert_ne!(p_coingecko, "0", "coingecko not refreshed");
    let pc: f64 = p_chainlink.parse().expect("chainlink price parse");
    let pg: f64 = p_coingecko.parse().expect("coingecko price parse");
    assert!(
        (0.9..=1.1).contains(&pc),
        "chainlink USDC out of range: {pc}"
    );
    assert!(
        (0.9..=1.1).contains(&pg),
        "coingecko USDC out of range: {pg}"
    );
}

#[tokio::test]
#[ignore]
async fn live_multicall_5_token_total_supplies() {
    use policy_sync::fetchers::rpc::multicall::{Call3, Multicall};
    use policy_sync::BlockTag;
    use std::sync::Arc;

    let router = Arc::new(RpcRouter::from_config(live_config()).unwrap());
    let mc = Multicall::new(router.clone());

    let totalsupply_selector = vec![0x18, 0x16, 0x0d, 0xdd]; // totalSupply()
    let tokens: Vec<(&str, &str)> = vec![
        ("USDC", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        ("USDT", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
        ("DAI", "0x6b175474e89094c44da98b954eedeac495271d0f"),
        ("WETH", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
        ("WBTC", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
    ];

    use std::str::FromStr as _;
    let calls: Vec<Call3> = tokens
        .iter()
        .map(|(_, addr)| Call3 {
            target: alloy_primitives::Address::from_str(addr).unwrap(),
            allow_failure: true,
            call_data: totalsupply_selector.clone(),
        })
        .collect();

    let results = mc
        .aggregate3(&ChainId::ethereum_mainnet(), calls, BlockTag::Latest)
        .await
        .expect("multicall aggregate3");

    assert_eq!(results.len(), 5);
    for ((name, _), result) in tokens.iter().zip(results.iter()) {
        assert!(result.success, "{} failed", name);
        assert_eq!(result.return_data.len(), 32);
        let supply = alloy_primitives::U256::from_be_slice(&result.return_data);
        println!("{} totalSupply = {}", name, supply);
        assert!(supply > alloy_primitives::U256::ZERO);
    }
}

#[tokio::test]
#[ignore]
async fn live_orchestrator_refresh_end_to_end() {
    use policy_state::{
        Address, Balance, BaseCategory, DataSource, Decimal, Duration as SDuration, FiatCurrency,
        LiveField, OracleProvider, PegTarget, Time, TokenHolding, TokenKey, TokenKind, WalletId,
        WalletState,
    };
    use policy_sync::Orchestrator;
    use std::str::FromStr;

    let orch = Orchestrator::from_sync_config(&live_sync_config()).unwrap();

    let usdc_addr = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
    let usdc_key = TokenKey::Erc20 {
        chain: ChainId::ethereum_mainnet(),
        address: usdc_addr,
    };

    let mut state = WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
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
            price_usd: Some(
                LiveField::new(
                    Decimal::new("999.99"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(1),
                )
                .with_ttl(SDuration::from_secs(1)),
            ),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1),
            primitives_source: DataSource::UserSupplied,
        },
    );

    let before_value = state.tokens[&usdc_key]
        .price_usd
        .as_ref()
        .unwrap()
        .value
        .as_str()
        .to_string();
    println!("before: USDC price = {}", before_value);
    assert_eq!(before_value, "999.99");

    let report = orch
        .refresh(&mut state, Time::from_unix(1_738_000_000))
        .await
        .unwrap();
    println!("refresh report: {:?}", report);

    let after = state.tokens[&usdc_key].price_usd.as_ref().unwrap();
    println!(
        "after:  USDC price = {} (synced_at={})",
        after.value.as_str(),
        after.synced_at.as_unix()
    );

    assert_ne!(
        after.value.as_str(),
        "999.99",
        "price should have been refreshed"
    );
    assert_eq!(after.synced_at, Time::from_unix(1_738_000_000));
    assert_eq!(report.fields_updated, 1);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
}

#[tokio::test]
#[ignore]
async fn live_aave_borrow_scenario_fills_live_inputs() {
    //!   ✓ asset_price_usd       — Chainlink USDC/USD
    use policy_state::{
        Address, DataSource, Decimal, Duration as SDuration, LiveField, OracleProvider, Price,
        RateMode, Time, TokenKey, TokenRef, WalletId, WalletState, U256,
    };
    use policy_sync::Orchestrator;
    use policy_transition::action::lending::{
        BorrowAction, BorrowLiveInputs, LendingAction, LendingVenue, ReserveState, UserLendingState,
    };
    use policy_transition::action::{Action, ActionBody, ActionMeta, ActionNature};
    use std::str::FromStr;

    let chain = ChainId::ethereum_mainnet();
    let aave_pool = Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(); // Aave V3 Pool
    let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
    let vitalik = Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap();

    fn empty_reserve() -> ReserveState {
        ReserveState {
            total_supply: U256::ZERO,
            total_borrow: U256::ZERO,
            utilization_bp: 0,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 0,
            liquidation_threshold_bp: 0,
            liquidation_bonus_bp: 0,
            reserve_factor_bp: 0,
            is_frozen: false,
            is_paused: false,
        }
    }
    fn empty_user() -> UserLendingState {
        UserLendingState {
            health_factor: Decimal::from("0"),
            total_collat_usd: U256::ZERO,
            total_debt_usd: U256::ZERO,
            available_borrow_usd: U256::ZERO,
        }
    }

    let stale = Time::from_unix(1);

    let borrow = BorrowAction {
        venue: LendingVenue::AaveV3 {
            chain: chain.clone(),
            pool: aave_pool,
            market_id: None,
        },
        asset: TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: usdc,
            },
        },
        amount: U256::from(500_000_000u64), // 500 USDC
        rate_mode: RateMode::Variable,
        on_behalf_of: None,
        live_inputs: BorrowLiveInputs {
            reserve_state: LiveField::new(
                empty_reserve(),
                DataSource::OnchainView {
                    chain: chain.clone(),
                    contract: aave_pool,
                    function: "getReserveData(address)".into(),
                    decoder_id: "aave_v3_reserve_data".into(),
                },
                stale,
            )
            .with_ttl(SDuration::from_secs(60)),
            user_state_before: LiveField::new(
                empty_user(),
                DataSource::OnchainView {
                    chain: chain.clone(),
                    contract: aave_pool,
                    function: "getUserAccountData(address)".into(),
                    decoder_id: "aave_v3_user_account_data".into(),
                },
                stale,
            )
            .with_ttl(SDuration::from_secs(60)),
            // ✓ Chainlink USDC/USD
            asset_price_usd: LiveField::new(
                Price::from("0"),
                DataSource::OracleFeed {
                    provider: OracleProvider::Chainlink,
                    feed_id: "USDC/USD".into(),
                },
                stale,
            )
            .with_ttl(SDuration::from_secs(60)),
            current_borrow_rate: LiveField::new(
                Decimal::from("0"),
                DataSource::OnchainView {
                    chain: chain.clone(),
                    contract: aave_pool,
                    function: "getReserveData(address)".into(),
                    decoder_id: "aave_v3_current_borrow_rate".into(),
                },
                stale,
            )
            .with_ttl(SDuration::from_secs(60)),
            available_liquidity: LiveField::new(
                U256::ZERO,
                DataSource::OnchainView {
                    chain: chain.clone(),
                    contract: usdc,
                    function: "balanceOf(address)".into(),
                    decoder_id: "erc20_balance".into(),
                },
                stale,
            )
            .with_ttl(SDuration::from_secs(60)),
        },
    };

    let mut action = Action {
        meta: ActionMeta {
            submitted_at: stale,
            submitter: vitalik,
            nature: ActionNature::OnchainTx {
                chain: chain.clone(),
                nonce: 0,
                gas_limit: U256::from(350_000u64),
                gas_price: LiveField::new(U256::ZERO, DataSource::UserSupplied, stale),
                value: U256::ZERO,
            },
        },
        body: ActionBody::Lending(LendingAction::Borrow(borrow)),
    };

    let state = WalletState::new(WalletId::new(vitalik, [chain.clone()]));

    let orch = Orchestrator::from_sync_config(&live_sync_config()).unwrap();

    let now = Time::from_unix(1_738_000_000);
    let report = orch.refresh_action(&mut action, &state, now).await.unwrap();
    println!("aave borrow refresh report: {:?}", report);

    if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
        let li = &b.live_inputs;
        println!(
            "[1] asset_price_usd       value={} synced={}",
            li.asset_price_usd.value.as_str(),
            li.asset_price_usd.synced_at.as_unix()
        );
        println!(
            "[2] available_liquidity   value={} synced={}",
            li.available_liquidity.value,
            li.available_liquidity.synced_at.as_unix()
        );
        println!(
            "[3] user_state_before     hf={} synced={}",
            li.user_state_before.value.health_factor.as_str(),
            li.user_state_before.synced_at.as_unix()
        );
        println!(
            "[4] reserve_state         total_supply={} synced={}",
            li.reserve_state.value.total_supply,
            li.reserve_state.synced_at.as_unix()
        );
        println!(
            "[5] current_borrow_rate   value={} synced={}",
            li.current_borrow_rate.value.as_str(),
            li.current_borrow_rate.synced_at.as_unix()
        );

        assert_ne!(
            li.asset_price_usd.value.as_str(),
            "0",
            "asset_price_usd should be filled"
        );
        assert!(
            li.available_liquidity.value > U256::ZERO,
            "available_liquidity > 0"
        );
    } else {
        panic!("expected Borrow action");
    }

    assert!(report.fields_updated >= 2);
}

#[test]
fn manifest_v2_parse_real_uniswap_universal_router() {
    use policy_sync::manifest_v2::{parse_live_inputs, resolve_placeholders, ResolveContext};
    use std::fs;

    let path =
        "../../../registryV2/manifests/uniswap/universal-router/execute-v1-no-deadline@1.0.0.json";
    let manifest_text = fs::read_to_string(path).expect("read manifest file");
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JSON");

    let live_subtree = &manifest_json["emit"]["per_opcode_body"]["0x00"]["body"]["amm"]["swap"];

    let parsed = parse_live_inputs(live_subtree).expect("parse live_inputs");
    println!("parsed {} live_input slots:", parsed.len());
    for (slot, spec) in &parsed {
        println!("  - {} (ttl={:?})", slot, spec.ttl_s);
    }

    assert!(parsed.contains_key("route"));
    assert!(parsed.contains_key("expected_amount_out"));
    assert!(parsed.contains_key("price_impact_bp"));
    assert!(parsed.contains_key("gas_estimate"));

    let route_source = &parsed["route"].source;
    assert_eq!(route_source["kind"], "onchain_view");
    assert_eq!(route_source["chain"], "$chain");
    assert_eq!(route_source["contract"], "$resolved.pool");
    assert_eq!(route_source["function"], "slot0()");

    let ctx = ResolveContext::new()
        .with_chain("eip155:1")
        .insert_resolved(
            "pool",
            serde_json::json!("0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640"),
        );

    let resolved = resolve_placeholders(route_source, &ctx).unwrap();
    assert_eq!(resolved["chain"], "eip155:1");
    assert_eq!(
        resolved["contract"],
        "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640"
    );
    println!("\nresolved route source:");
    println!("{}", serde_json::to_string_pretty(&resolved).unwrap());

    let gas_source = &parsed["gas_estimate"].source;
    assert_eq!(gas_source["kind"], "oracle_feed");
    assert_eq!(gas_source["provider"], "pyth");
}
