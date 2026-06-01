//! Phase 3 — `/dashboard/summary` + `/wallets/:addr/approvals?with_risk=true`.

use std::str::FromStr;
use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_state::approval::{AllowanceSpec, ApprovalSet};
use simulation_state::primitives::{Address, ChainId, Time, U256};
use simulation_state::token::{Balance, TokenHolding, TokenKey, TokenKind};
use simulation_state::{WalletId, WalletState, WalletStore};
use simulation_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "test@example.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (
    std::net::SocketAddr,
    MultiUserStore,
    tempfile::TempDir,
    String,
) {
    let tmp = tempfile::tempdir().unwrap();
    let global_db = GlobalDb::open(tmp.path().join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(tmp.path().join("users"));
    let state = AppState {
        multi_user: multi_user.clone(),
        global_db,
        event_bus: simulation_server::events::EventBus::new(),
        orchestrator: Arc::new(Orchestrator::from_sync_config(&SyncConfig::default()).unwrap()),
        etherscan: None,
        coingecko: simulation_sync::CoinGeckoClient::new(),
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let token = mint_token("u_test_alice");
    (addr, multi_user, tmp, token)
}

const WALLET_ADDR: &str = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045";

fn usdc_holding_with_value(amount_units: u128, price: &str) -> TokenHolding {
    use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
    use simulation_state::primitives::{Duration, Price};

    let key = TokenKey::Erc20 {
        chain: ChainId::ethereum_mainnet(),
        address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
    };
    TokenHolding {
        key: key.clone(),
        kind: TokenKind::Unknown,
        symbol: "USDC".into(),
        decimals: 6,
        balance: Balance::Fungible {
            amount: U256::from(amount_units),
        },
        committed: Balance::zero_fungible(),
        approved_to: None,
        price_usd: Some(
            LiveField::new(
                Price::new(price),
                DataSource::OracleFeed {
                    provider: OracleProvider::Chainlink,
                    feed_id: "USDC/USD".into(),
                },
                Time::from_unix(1_730_000_000),
            )
            .with_ttl(Duration::from_secs(60)),
        ),
        metadata: None,
        value_usd: None,
        last_synced_at: Time::from_unix(1_730_000_000),
        primitives_source: DataSource::UserSupplied,
    }
}

async fn seed_state_with_holding(multi_user: &MultiUserStore, user_id: &str) {
    let store = multi_user.for_user(user_id).unwrap();
    let id = WalletId::new(
        Address::from_str(WALLET_ADDR).unwrap(),
        [ChainId::ethereum_mainnet()],
    );
    let mut s = WalletState::new(id);
    let h = usdc_holding_with_value(10_000_000_000, "1.0001"); // 10000 USDC × $1.0001
    s.tokens.insert(h.key.clone(), h);
    // Seed an unlimited ERC20 approval to Uniswap V3 router so the
    // risk classifier has something to mark KNOWN_VENUE + UNLIMITED.
    let router = Address::from_str("0xe592427a0aece92de3edee1f18e0157c05861564").unwrap();
    let mut per_spender = std::collections::BTreeMap::new();
    per_spender.insert(
        router,
        AllowanceSpec {
            amount: U256::MAX,
            is_unlimited: true,
            last_set_at: Time::from_unix(1_730_000_000),
        },
    );
    let mut approvals = ApprovalSet::default();
    approvals.erc20.insert(
        (
            ChainId::ethereum_mainnet(),
            Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        ),
        per_spender,
    );
    s.approvals = approvals;
    store.save(&s).await.unwrap();
}

#[tokio::test]
async fn dashboard_summary_aggregates_portfolio() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    seed_state_with_holding(&mu, "u_test_alice").await;

    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/dashboard/summary"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["wallet_count"], 1);
    let total: f64 = body["total_portfolio_usd"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(total > 9999.0 && total < 10002.0, "total {total}");
    let chains = body["chain_breakdown"].as_array().unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0]["chain"], "eip155:1");
    let pct = chains[0]["pct"].as_f64().unwrap();
    assert!(pct > 99.0);
    let wallets = body["wallets"].as_array().unwrap();
    assert_eq!(wallets.len(), 1);
    assert_eq!(wallets[0]["address"], WALLET_ADDR);
    assert_eq!(wallets[0]["unlimited_count"], 1);
    assert_eq!(wallets[0]["pending_count"], 0);
}

#[tokio::test]
async fn dashboard_summary_empty_when_no_wallets() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/dashboard/summary"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["wallet_count"], 0);
    assert_eq!(body["total_portfolio_usd"], "0.000000");
    assert!(body["chain_breakdown"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn approvals_with_risk_returns_classified_shape() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    seed_state_with_holding(&mu, "u_test_alice").await;

    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "http://{addr}/wallets/{WALLET_ADDR}/approvals?with_risk=true"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let erc20 = body["erc20"].as_array().unwrap();
    assert_eq!(erc20.len(), 1);
    let row = &erc20[0];
    let risk: Vec<&str> = row["risk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    // Spender catalog removed → only UNLIMITED remains (KNOWN_VENUE
    // tag and spender_meta field no longer emitted).
    assert!(risk.contains(&"UNLIMITED"));
    assert!(row.get("spender_meta").is_none());
}

#[tokio::test]
async fn approvals_default_returns_raw_shape() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    seed_state_with_holding(&mu, "u_test_alice").await;

    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/wallets/{WALLET_ADDR}/approvals"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Raw ApprovalSet shape: erc20 is a map keyed by (chain, token).
    // Not the classified array. Just check we don't see the `risk` field.
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains("\"risk\""));
}
