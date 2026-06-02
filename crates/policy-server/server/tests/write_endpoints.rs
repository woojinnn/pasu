//! Integration: POST /wallets and POST /wallets/:addr/sync.
//! Uses an empty `SyncConfig`, so the orchestrator has no providers and
//! `refresh()` is essentially a no-op (no LiveFields to walk, nothing
//! to fetch). The tests therefore verify the HTTP plumbing — add wallet
//! persists, sync is reachable, errors map to the right status — not
//! the actual RPC integration, which lives under `tests/sync_integration.rs`
//! (ignored by default; requires network).

use std::sync::Arc;

use axum::routing::post;
use axum::{Json, Router};
use policy_db::{GlobalDb, MultiUserStore};
use policy_server::app::{build_router, AppState};
use policy_server::auth::jwt::{issue, TokenType};
use policy_server::events::{EventBus, LocalEventPublisher};
use policy_state::{Decimal, PositionKind, WalletId, WalletState, WalletStore};
use policy_sync::{HyperliquidConfig, Orchestrator, SyncConfig};
use serde_json::{json, Value};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "x@e.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (std::net::SocketAddr, MultiUserStore, String) {
    spawn_server_with_orchestrator(Orchestrator::from_sync_config(&SyncConfig::default()).unwrap())
        .await
}

async fn spawn_server_with_orchestrator(
    orchestrator: Orchestrator,
) -> (std::net::SocketAddr, MultiUserStore, String) {
    ensure_jwt_secret();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.keep();
    let global_db = GlobalDb::open(path.join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(path.join("users"));
    let event_bus = EventBus::new();
    let state = AppState {
        multi_user: multi_user.clone(),
        global_db,
        event_bus: event_bus.clone(),
        publisher: Arc::new(LocalEventPublisher::new(event_bus)),
        orchestrator: Arc::new(orchestrator),
        etherscan: None,
        coingecko: policy_sync::CoinGeckoClient::new(),
        coordinator: Arc::new(policy_server::coordination::NoopCoordinator),
        sync_lock_ttl: std::time::Duration::from_secs(120),
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let token = mint_token("u_write_alice");
    (addr, multi_user, token)
}

async fn spawn_hyperliquid_info_server(withdrawable: &'static str) -> String {
    let app = Router::new().route(
        "/info",
        post(move |Json(req): Json<Value>| async move {
            Json(hyperliquid_info_response(&req, withdrawable))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn hyperliquid_info_response(req: &Value, withdrawable: &str) -> Value {
    match req["type"].as_str().unwrap_or_default() {
        "clearinghouseState" => json!({
            "marginSummary": {
                "accountValue": withdrawable,
                "totalNtlPos": "0",
                "totalRawUsd": withdrawable,
                "totalMarginUsed": "0"
            },
            "crossMarginSummary": {
                "accountValue": withdrawable,
                "totalNtlPos": "0",
                "totalRawUsd": withdrawable,
                "totalMarginUsed": "0"
            },
            "withdrawable": withdrawable,
            "assetPositions": [],
            "time": 1_710_000_000_123_u64
        }),
        "frontendOpenOrders" | "extraAgents" | "perpDexs" | "delegations" | "userVaultEquities" => {
            json!([])
        }
        "spotClearinghouseState" => json!({
            "balances": [],
            "tokenToAvailableAfterMaintenance": []
        }),
        "delegatorSummary" => json!({
            "delegated": "0",
            "undelegated": "0",
            "totalPendingWithdrawal": "0",
            "nPendingWithdrawals": 0
        }),
        "borrowLendUserState" => json!({
            "tokenToState": [],
            "health": "healthy",
            "healthFactor": null
        }),
        "meta" => json!({
            "universe": [],
            "collateralToken": 0
        }),
        other => panic!("unexpected Hyperliquid info request: {other}"),
    }
}

fn hyperliquid_perp_usdc(state: &WalletState) -> Option<Decimal> {
    state
        .positions
        .iter()
        .find_map(|position| match &position.kind {
            PositionKind::HyperliquidAccount(account) => account.perp_usdc.clone(),
            _ => None,
        })
}

async fn spawn_server_with_hyperliquid(
    withdrawable: &'static str,
) -> (std::net::SocketAddr, MultiUserStore, String) {
    let endpoint = spawn_hyperliquid_info_server(withdrawable).await;
    let mut sync_config = SyncConfig::default();
    sync_config.venues.hyperliquid = Some(HyperliquidConfig { endpoint });
    spawn_server_with_orchestrator(Orchestrator::from_sync_config(&sync_config).unwrap()).await
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn post_wallets_persists_and_returns_id() {
    let (addr, mu, token) = spawn_server().await;
    let body = serde_json::json!({
        "address": "0x000000000000000000000000000000000000a01c",
        "chains": ["eip155:1"],
        "label": "main",
    });

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let parsed: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        parsed["wallet_id"]["address"]
            .as_str()
            .unwrap()
            .to_lowercase(),
        "0x000000000000000000000000000000000000a01c"
    );

    // Direct store check — the wallet row landed in the user's DB.
    let store = mu.for_user("u_write_alice").unwrap();
    let wallets = store.list_wallets().await.unwrap();
    assert_eq!(wallets.len(), 1);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn post_wallets_runs_hyperliquid_account_sync() {
    let (addr, mu, token) = spawn_server_with_hyperliquid("123.45").await;
    let body = serde_json::json!({
        "address": "0x000000000000000000000000000000000000a01c",
        "chains": ["eip155:1"],
        "label": "main",
    });

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let store = mu.for_user("u_write_alice").unwrap();
    let wallets = store.list_wallets().await.unwrap();
    let state = store.load(&wallets[0]).await.unwrap();
    assert_eq!(hyperliquid_perp_usdc(&state), Some(Decimal::new("123.45")));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn post_wallets_rejects_when_no_chains_configurable() {
    // Test setup uses an empty SyncConfig — the router has zero
    // chains. Empty `chains: []` triggers the auto-default path,
    // which also yields zero chains here, so we expect 400 with a
    // clear error rather than a silent successful add against
    // nothing.
    let (addr, _mu, token) = spawn_server().await;
    let body = serde_json::json!({
        "address": "0x000000000000000000000000000000000000a01c",
        "chains": [],
    });
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn post_wallets_rejects_bad_address() {
    let (addr, _mu, token) = spawn_server().await;
    let body = serde_json::json!({
        "address": "not-an-address",
        "chains": ["eip155:1"],
    });
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn sync_unknown_wallet_returns_404() {
    let (addr, _mu, token) = spawn_server().await;
    let other = "0x0000000000000000000000000000000000001111";
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets/{other}/sync"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn sync_known_wallet_returns_204() {
    let (addr, mu, token) = spawn_server().await;
    // Pre-seed a wallet so /sync has something to refresh.
    let store = mu.for_user("u_write_alice").unwrap();
    let id = WalletId::new(
        std::str::FromStr::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
        [policy_state::primitives::ChainId::ethereum_mainnet()],
    );
    store
        .save(&policy_state::WalletState::new(id.clone()))
        .await
        .unwrap();

    let addr_lower = format!("{:#x}", id.address);
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets/{addr_lower}/sync"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn sync_known_wallet_runs_hyperliquid_account_sync() {
    let (addr, mu, token) = spawn_server_with_hyperliquid("456.78").await;
    let store = mu.for_user("u_write_alice").unwrap();
    let id = WalletId::new(
        std::str::FromStr::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
        [policy_state::primitives::ChainId::ethereum_mainnet()],
    );
    store.save(&WalletState::new(id.clone())).await.unwrap();

    let addr_lower = format!("{:#x}", id.address);
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/wallets/{addr_lower}/sync"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let state = store.load(&id).await.unwrap();
    assert_eq!(hyperliquid_perp_usdc(&state), Some(Decimal::new("456.78")));
}
