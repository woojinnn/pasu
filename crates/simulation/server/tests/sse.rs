//! Integration: `GET /events/stream` (SSE).
//!
//! Spawns the server, opens an SSE connection, publishes events via the
//! shared bus, and asserts the right user receives the right events.
//! Verifies tenant isolation — alice never sees bob's events.

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_server::events::types::{Event, TxConfirmed};
use simulation_server::events::EventBus;
use simulation_state::primitives::ChainId;
use simulation_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "x@e.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (std::net::SocketAddr, EventBus) {
    ensure_jwt_secret();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.keep();
    let global_db = GlobalDb::open(path.join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(path.join("users"));
    let bus = EventBus::new();
    let state = AppState {
        multi_user,
        global_db,
        event_bus: bus.clone(),
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
    (addr, bus)
}

fn sample_event(tx_id: &str) -> Event {
    Event::TxConfirmed(TxConfirmed {
        tx_id: tx_id.into(),
        wallet: "0xowner".into(),
        chain: ChainId::ethereum_mainnet(),
        tx_hash: "0xdeadbeef".into(),
        block_number: 19_000_000,
        success: true,
    })
}

/// Connect to SSE and read until we see at least `n` `event:` lines or
/// `timeout` elapses. Returns the raw body text we accumulated.
async fn read_n_events(
    addr: std::net::SocketAddr,
    token: &str,
    n: usize,
    timeout: std::time::Duration,
) -> String {
    let url = format!("http://{addr}/events/stream");
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    use futures::StreamExt;
    let mut stream = Box::pin(resp.bytes_stream());
    let mut buf = String::new();
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        let n_so_far = buf.lines().filter(|l| l.starts_with("event: ")).count();
        if n_so_far >= n {
            return buf;
        }
        match tokio::time::timeout(std::time::Duration::from_millis(200), stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                buf.push_str(std::str::from_utf8(&chunk).unwrap_or(""));
            }
            Ok(Some(Err(_))) | Ok(None) => return buf,
            Err(_) => {} // tick timeout — keep waiting on the loop guard
        }
    }
    buf
}

#[tokio::test]
async fn subscriber_receives_published_event() {
    let (addr, bus) = spawn_server().await;
    let token = mint_token("u_alice");

    // Start the SSE reader, then publish a few events.
    let reader = tokio::spawn(async move {
        read_n_events(addr, &token, 1, std::time::Duration::from_secs(3)).await
    });

    // Give the reader a moment to subscribe before we publish.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    bus.publish("u_alice", sample_event("t1"));

    let body = reader.await.unwrap();
    assert!(body.contains("event: tx_confirmed"), "body=\n{body}");
    assert!(body.contains("\"tx_id\":\"t1\""), "body=\n{body}");
}

#[tokio::test]
async fn other_users_events_are_filtered_out() {
    let (addr, bus) = spawn_server().await;
    let alice_token = mint_token("u_alice");

    let reader = tokio::spawn(async move {
        read_n_events(addr, &alice_token, 1, std::time::Duration::from_millis(800)).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    bus.publish("u_bob", sample_event("t_for_bob"));

    let body = reader.await.unwrap();
    // Alice's stream should not have seen bob's event. The body may
    // contain the `:connected` prelude comment but no `event:` lines.
    assert!(!body.contains("event: "), "alice saw bob's event:\n{body}");
}

#[tokio::test]
async fn missing_token_rejected_with_401() {
    let (addr, _bus) = spawn_server().await;
    let resp = reqwest::get(format!("http://{addr}/events/stream"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
