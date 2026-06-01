//! Integration: read-only endpoints (`GET /wallets…`) served against the
//! PostgreSQL wallet store the simulator writes through. Seeds a
//! wallet end-to-end and asserts every endpoint returns the expected slice.
//!
//! Every request carries a Bearer JWT minted from the same `JWT_SECRET`
//! the server reads.

use std::str::FromStr;
use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_server::events::{EventBus, LocalEventPublisher};
use simulation_state::approval::{AllowanceSpec, ApprovalSet};
use simulation_state::primitives::{Address, BlockHeight, ChainId, Time, U256};
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

fn sample_id() -> WalletId {
    WalletId::new(
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
        [ChainId::ethereum_mainnet()],
    )
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
    let event_bus = EventBus::new();
    let state = AppState {
        multi_user: multi_user.clone(),
        global_db,
        event_bus: event_bus.clone(),
        publisher: Arc::new(LocalEventPublisher::new(event_bus)),
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

fn seeded_state() -> WalletState {
    let id = sample_id();
    let mut s = WalletState::new(id);
    s.block_heights.insert(
        ChainId::ethereum_mainnet(),
        BlockHeight {
            number: 19_500_000,
            time: 1_700_000_000,
        },
    );
    let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
    let spender = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let mut per_spender = std::collections::BTreeMap::new();
    per_spender.insert(
        spender,
        AllowanceSpec {
            amount: U256::from(1_000_000u64),
            is_unlimited: false,
            last_set_at: Time::from_unix(1_700_000_000),
        },
    );
    let mut erc20 = std::collections::BTreeMap::new();
    erc20.insert((ChainId::ethereum_mainnet(), usdc), per_spender);
    s.approvals = ApprovalSet {
        erc20,
        ..ApprovalSet::default()
    };
    s
}

async fn seed_for(multi_user: &MultiUserStore, user_id: &str, state: WalletState) {
    multi_user
        .for_user(user_id)
        .unwrap()
        .save(&state)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn list_wallets_returns_seeded() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    seed_for(&mu, "u_test_alice", seeded_state()).await;

    let listed: Vec<WalletId> = reqwest::Client::new()
        .get(format!("http://{addr}/wallets"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed, vec![sample_id()]);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn get_state_returns_full_wallet() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    let seed = seeded_state();
    seed_for(&mu, "u_test_alice", seed.clone()).await;

    let lower = format!("{:#x}", sample_id().address);
    let got: WalletState = reqwest::Client::new()
        .get(format!("http://{addr}/wallets/{lower}/state"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(got, seed);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn get_approvals_returns_approval_set() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    seed_for(&mu, "u_test_alice", seeded_state()).await;

    let lower = format!("{:#x}", sample_id().address);
    let got: ApprovalSet = reqwest::Client::new()
        .get(format!("http://{addr}/wallets/{lower}/approvals"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(got.erc20.len(), 1);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn get_block_heights_returns_array() {
    let (addr, mu, _tmp, token) = spawn_server().await;
    seed_for(&mu, "u_test_alice", seeded_state()).await;

    let lower = format!("{:#x}", sample_id().address);
    let body = reqwest::Client::new()
        .get(format!("http://{addr}/wallets/{lower}/block-heights"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"number\":19500000"), "body={body}");
    assert!(body.contains("\"chain\":\"eip155:1\""), "body={body}");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn invalid_address_returns_400() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/wallets/not-an-address/state"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn unseen_address_returns_empty_state() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let other = "0x0000000000000000000000000000000000001111";
    let got: WalletState = reqwest::Client::new()
        .get(format!("http://{addr}/wallets/{other}/state"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(got.tokens.is_empty());
    assert!(got.block_heights.is_empty());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn isolated_user_cannot_see_others_wallets() {
    let (addr, mu, _tmp, _token_alice) = spawn_server().await;
    seed_for(&mu, "u_test_alice", seeded_state()).await;

    let bob_token = mint_token("u_test_bob");
    let listed: Vec<WalletId> = reqwest::Client::new()
        .get(format!("http://{addr}/wallets"))
        .bearer_auth(&bob_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(listed.is_empty(), "bob saw alice's wallet: {listed:?}");
}
