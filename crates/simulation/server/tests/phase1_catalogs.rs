//! Phase 1 catalog endpoints — `/policies/:id`, `/policy-schema`,
//! `/policy-templates`, `/examples/transactions`, `/spenders/:addr`.
//!
//! Static-JSON endpoints don't touch the DB; they verify response
//! content-type + top-level shape. `/spenders` exercises the catalog
//! seeding path.

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_server::spenders::SpenderCatalog;
use simulation_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "test@example.com", TokenType::Access, None).unwrap()
}

async fn spawn_server(spenders: SpenderCatalog) -> (std::net::SocketAddr, tempfile::TempDir) {
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
        spenders,
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (addr, tmp)
}

fn empty_spenders() -> SpenderCatalog {
    SpenderCatalog::empty()
}

fn seeded_spenders() -> SpenderCatalog {
    let toml = r#"
[[spenders]]
addr  = "0xe592427a0aece92de3edee1f18e0157c05861564"
label = "Uniswap V3 SwapRouter"
rep   = "known"

[[spenders]]
addr  = "0x000000000000000000000000000000000000dead"
label = "burn (test)"
rep   = "blocked"
"#;
    let (cat, warnings) = SpenderCatalog::from_toml(toml).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    cat
}

#[tokio::test]
async fn policy_schema_serves_static_json() {
    let (addr, _tmp) = spawn_server(empty_spenders()).await;
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
    let (addr, _tmp) = spawn_server(empty_spenders()).await;
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
    let (addr, _tmp) = spawn_server(empty_spenders()).await;
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
async fn spender_lookup_known_and_unknown() {
    let (addr, _tmp) = spawn_server(seeded_spenders()).await;
    let token = mint_token("u_test_alice");

    // Known
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/spenders/0xe592427a0aece92de3edee1f18e0157c05861564"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let meta: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(meta["rep"], "known");
    assert_eq!(meta["label"], "Uniswap V3 SwapRouter");

    // Mixed-case address still resolves (server normalises to lower)
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/spenders/0xE592427A0AECE92DE3EDEE1F18E0157C05861564"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Unknown → 404
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/spenders/0x0000000000000000000000000000000000000123"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    // Bad address shape → 400
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/spenders/not-an-addr"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn policy_by_id_404_when_missing() {
    let (addr, _tmp) = spawn_server(empty_spenders()).await;
    let token = mint_token("u_test_alice");
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/policies/9999"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
