//! Integration: POST /wallets and POST /wallets/:addr/sync.
//!
//! Uses an empty `SyncConfig`, so the orchestrator has no providers and
//! `refresh()` is essentially a no-op (no LiveFields to walk, nothing
//! to fetch). The tests therefore verify the HTTP plumbing — add wallet
//! persists, sync is reachable, errors map to the right status — not
//! the actual RPC integration, which lives under `tests/sync_integration.rs`
//! (ignored by default; requires network).

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_state::{WalletId, WalletStore};
use simulation_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "x@e.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (std::net::SocketAddr, MultiUserStore, String) {
    ensure_jwt_secret();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.keep();
    let global_db = GlobalDb::open(path.join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(path.join("users"));
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
    let token = mint_token("u_write_alice");
    (addr, multi_user, token)
}

#[tokio::test]
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
async fn sync_known_wallet_returns_204() {
    let (addr, mu, token) = spawn_server().await;
    // Pre-seed a wallet so /sync has something to refresh.
    let store = mu.for_user("u_write_alice").unwrap();
    let id = WalletId::new(
        std::str::FromStr::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
        [simulation_state::primitives::ChainId::ethereum_mainnet()],
    );
    store
        .save(&simulation_state::WalletState::new(id.clone()))
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
