//! Phase 4 — Cedar editor support (`/policies/validate` + `/policies/:id/test`).

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
        spenders: SpenderCatalog::empty(),
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

#[tokio::test]
async fn validate_accepts_well_formed_policy() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/policies/validate"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "cedar_text": "permit(principal, action, resource);"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["ok"], true);
    assert!(body.get("error").is_none_or(|v| v.is_null()));
}

#[tokio::test]
async fn validate_rejects_garbage_with_error_message() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/policies/validate"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "cedar_text": "this is definitely not Cedar text }}}"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["ok"], false);
    assert!(body["error"].as_str().is_some());
    assert!(!body["error"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn validate_rejects_empty_string() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/policies/validate"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "cedar_text": "   " }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["ok"], false);
}

#[tokio::test]
async fn test_policy_404_when_missing() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/policies/9999/test"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "request": {
                "principal": "Wallet::\"0xabc\"",
                "action": "Action::\"swap\"",
                "resource": "Protocol::\"0xdef\"",
                "entities": [],
                "context": {}
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_policy_runs_against_saved_policy() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    // Install a wide-open permit policy first.
    let post: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/policies"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "permit-all",
            "cedar_text": "permit(principal, action, resource);",
            "severity": "info"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = post["id"].as_i64().unwrap();

    let raw = reqwest::Client::new()
        .post(format!("http://{addr}/policies/{id}/test"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "request": {
                "principal": "Wallet::\"0xabc\"",
                "action": "Action::\"swap\"",
                "resource": "Protocol::\"0xdef\"",
                "entities": [],
                "context": {}
            }
        }))
        .send()
        .await
        .unwrap();
    let status = raw.status();
    let text = raw.text().await.unwrap();
    assert!(status.is_success(), "status={status} body={text}");
    let resp: serde_json::Value = serde_json::from_str(&text).expect(&text);
    // Baseline-permit + permit-all → pass.
    assert_eq!(resp["verdict"], "pass");
    assert!(resp["matched"].as_array().unwrap().is_empty());
}
