//! Phase 1 catalog endpoints — `/policies/:id`, `/policy-schema`,
//! `/policy-templates`, `/examples/transactions`.
//!
//! Static-JSON endpoints don't touch the DB; we verify response
//! content-type + top-level shape. `/spenders/:addr` and its
//! catalog-seeding path were removed in the DB-only refactor — the
//! spender label catalog now lives outside the server.

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "test@example.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (std::net::SocketAddr, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let global_db = GlobalDb::open(tmp.path().join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(tmp.path().join("users"));
    let state = AppState {
        multi_user,
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
    (addr, tmp)
}

#[tokio::test]
async fn policy_schema_serves_static_json() {
    let (addr, _tmp) = spawn_server().await;
    let token = mint_token("u_test_alice");
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/policy-schema"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("application/json"));
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("actions").is_some());
    assert!(body.get("predicates").is_some());
    assert!(body.get("operators").is_some());
}

#[tokio::test]
async fn policy_templates_array() {
    let (addr, _tmp) = spawn_server().await;
    let token = mint_token("u_test_alice");
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/policy-templates"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body.is_array());
    let arr = body.as_array().unwrap();
    assert!(!arr.is_empty(), "templates seed is empty");
    let first = &arr[0];
    assert!(first.get("name").is_some());
    assert!(first.get("cedar_text").is_some());
    assert!(first.get("severity").is_some());
}

#[tokio::test]
async fn example_transactions_array() {
    let (addr, _tmp) = spawn_server().await;
    let token = mint_token("u_test_alice");
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/examples/transactions"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body.is_array());
    assert!(!body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn policy_by_id_404_when_missing() {
    let (addr, _tmp) = spawn_server().await;
    let token = mint_token("u_test_alice");
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/policies/9999"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
