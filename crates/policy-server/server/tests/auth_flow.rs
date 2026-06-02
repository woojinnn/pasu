//! Integration: auth surface end-to-end.
//! Doesn't drive a real Google login (external dependency); instead mints
//! tokens with the same `JWT_SECRET` the server reads and exercises the
//! middleware via real HTTP.

use std::sync::Arc;

use policy_db::{GlobalDb, MultiUserStore};
use policy_server::app::{build_router, build_router_with_config, AppState};
use policy_server::auth::jwt::{issue, verify, TokenType};
use policy_server::config::ServerConfig;
use policy_server::events::{EventBus, LocalEventPublisher};
use policy_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

async fn spawn_server() -> std::net::SocketAddr {
    ensure_jwt_secret();
    let tmp = tempfile::tempdir().unwrap();
    // Leak the tempdir into the spawned server's lifetime — these tests
    // don't outlive the runtime, and Drop on the dir while serving is fine.
    let path = tmp.keep();
    let global_db = GlobalDb::open(path.join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(path.join("users"));
    let event_bus = EventBus::new();
    let state = AppState {
        multi_user,
        global_db,
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
    addr
}

async fn spawn_server_with_origin_allowlist(origins: Vec<&str>) -> std::net::SocketAddr {
    ensure_jwt_secret();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.keep();
    let global_db = GlobalDb::open(path.join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(path.join("users"));
    let event_bus = EventBus::new();
    let state = AppState {
        multi_user,
        global_db,
        event_bus: event_bus.clone(),
        publisher: Arc::new(LocalEventPublisher::new(event_bus)),
        orchestrator: Arc::new(Orchestrator::from_sync_config(&SyncConfig::default()).unwrap()),
        etherscan: None,
        coingecko: policy_sync::CoinGeckoClient::new(),
    };
    let mut config = ServerConfig::for_tests();
    config.cors_allowed_origins = origins.into_iter().map(str::to_owned).collect();
    let router = build_router_with_config(state, &config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    addr
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn no_token_yields_401_with_json_error() {
    let addr = spawn_server().await;
    let resp = reqwest::get(format!("http://{addr}/wallets"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "unauthorized");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn malformed_authorization_yields_401() {
    let addr = spawn_server().await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/wallets"))
        .header("Authorization", "Token foo.bar.baz")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn expired_token_yields_401() {
    ensure_jwt_secret();
    let addr = spawn_server().await;
    let expired = issue("u_x", "x@e.com", TokenType::Access, Some(-10)).unwrap();
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/wallets"))
        .bearer_auth(&expired)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn refresh_token_cannot_access_protected_routes() {
    ensure_jwt_secret();
    let addr = spawn_server().await;
    let refresh = issue("u_x", "x@e.com", TokenType::Refresh, None).unwrap();
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/wallets"))
        .bearer_auth(&refresh)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn refresh_token_mints_new_access_token() {
    ensure_jwt_secret();
    let addr = spawn_server().await;
    let refresh = issue("u_x", "x@example.com", TokenType::Refresh, None).unwrap();

    let res = reqwest::Client::new()
        .post(format!("http://{addr}/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let json: serde_json::Value = res.json().await.unwrap();
    let access = json["access_token"].as_str().unwrap();
    let claims = verify(access).unwrap();
    assert_eq!(claims.sub, "u_x");
    assert!(claims.is_access());
    assert!(json["refresh_token"].as_str().is_some());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn valid_access_token_reaches_handler() {
    ensure_jwt_secret();
    let addr = spawn_server().await;
    let token = issue("u_x", "x@e.com", TokenType::Access, None).unwrap();
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/wallets"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn health_is_public_without_token() {
    let addr = spawn_server().await;
    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn google_redirect_when_env_configured() {
    ensure_jwt_secret();
    std::env::set_var("GOOGLE_CLIENT_ID", "test-client-id");
    std::env::set_var(
        "GOOGLE_REDIRECT_URI",
        "http://127.0.0.1:8788/auth/google/callback",
    );
    let addr = spawn_server().await;
    let resp = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("http://{addr}/auth/google"))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_redirection(),
        "expected redirect, got {}",
        resp.status()
    );
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("accounts.google.com"), "loc={location}");
    assert!(location.contains("client_id=test-client-id"));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn cors_rejects_unconfigured_origin() {
    let addr = spawn_server_with_origin_allowlist(vec!["https://app.scopeball.dev"]).await;
    let res = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, format!("http://{addr}/auth/me"))
        .header("origin", "https://evil.example")
        .header("access-control-request-method", "GET")
        .send()
        .await
        .unwrap();

    assert!(
        res.headers().get("access-control-allow-origin").is_none(),
        "unconfigured origins must not receive CORS approval"
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn cors_allows_configured_dashboard_origin() {
    let addr = spawn_server_with_origin_allowlist(vec!["https://app.scopeball.dev"]).await;
    let res = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, format!("http://{addr}/auth/me"))
        .header("origin", "https://app.scopeball.dev")
        .header("access-control-request-method", "GET")
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.headers()
            .get("access-control-allow-origin")
            .unwrap()
            .to_str()
            .unwrap(),
        "https://app.scopeball.dev"
    );
}
