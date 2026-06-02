//! Policy installation and verdict history are extension-local concerns.

use std::sync::Arc;

use policy_db::{GlobalDb, MultiUserStore};
use policy_server::app::{build_router, AppState};
use policy_server::auth::jwt::{issue, TokenType};
use policy_server::events::{EventBus, LocalEventPublisher};
use policy_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-06-01";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token() -> String {
    ensure_jwt_secret();
    issue("u_routes", "routes@example.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (std::net::SocketAddr, String) {
    ensure_jwt_secret();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.keep();
    let event_bus = EventBus::new();
    let state = AppState {
        multi_user: MultiUserStore::new(path.join("users")),
        global_db: GlobalDb::open(path.join("global.db")).unwrap(),
        event_bus: event_bus.clone(),
        publisher: Arc::new(LocalEventPublisher::new(event_bus)),
        orchestrator: Arc::new(Orchestrator::from_sync_config(&SyncConfig::default()).unwrap()),
        etherscan: None,
        coingecko: policy_sync::CoinGeckoClient::new(),
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (addr, mint_token())
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn policy_routes_are_not_served_by_policy_server() {
    let (addr, token) = spawn_server().await;
    let client = reqwest::Client::new();

    let requests = [
        client.get(format!("http://{addr}/policies")),
        client.post(format!("http://{addr}/policies")),
        client.get(format!("http://{addr}/policies/1")),
        client.patch(format!("http://{addr}/policies/1")),
        client.delete(format!("http://{addr}/policies/1")),
        client.get(format!("http://{addr}/policy-schema")),
        client.get(format!("http://{addr}/policy-templates")),
        client.get(format!("http://{addr}/examples/transactions")),
    ];

    for request in requests {
        let resp = request
            .bearer_auth(&token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404, "unexpected route response");
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn verdict_audit_and_finding_routes_are_not_served_by_policy_server() {
    let (addr, token) = spawn_server().await;
    let client = reqwest::Client::new();

    let requests = [
        client.post(format!("http://{addr}/verdicts")),
        client.patch(format!("http://{addr}/verdicts/1")),
        client.get(format!("http://{addr}/audit/verdicts")),
        client.get(format!("http://{addr}/audit/counts")),
        client.get(format!("http://{addr}/audit/export")),
        client.get(format!("http://{addr}/history/verdicts")),
        client.get(format!("http://{addr}/findings/feed")),
    ];

    for request in requests {
        let resp = request
            .bearer_auth(&token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404, "unexpected route response");
    }
}
