//! Integration: the axum server backed by per-user PostgreSQL wallet stores.
//! Spins up the router on an ephemeral port, hits `POST /evaluate` against
//! a tempdir-backed user DB, and asserts the state actually persists across
//! requests (the load→reduce→save path runs through PostgreSQL, not memory).
//! one with the same `JWT_SECRET` the server reads.

use std::str::FromStr;
use std::sync::Arc;

use policy_db::{GlobalDb, MultiUserStore};
use policy_server::app::{build_router, AppState};
use policy_server::auth::jwt::{issue, TokenType};
use policy_server::dto::{EvaluateRequest, EvaluateResponse};
use policy_server::events::{EventBus, LocalEventPublisher};
use policy_state::primitives::{Address, BlockHeight, ChainId, Time};
use policy_state::{EvalContext, RequestKind, WalletId, WalletState, WalletStore};
use policy_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn ensure_jwt_secret() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
}

fn mint_token(user_id: &str) -> String {
    ensure_jwt_secret();
    issue(user_id, "test@example.com", TokenType::Access, None).unwrap()
}

fn sample_wallet_id() -> WalletId {
    WalletId::new(
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
        [ChainId::ethereum_mainnet()],
    )
}

fn empty_envelope_request(id: WalletId) -> EvaluateRequest {
    EvaluateRequest {
        wallet_id: id,
        envelopes: Vec::new(),
        eval_context: EvalContext::new(
            ChainId::ethereum_mainnet(),
            Time::from_unix(1_700_000_000),
            RequestKind::Transaction,
        ),
        call_specs: Vec::new(),
    }
}

/// Build an AppState backed by PostgreSQL stores.
/// Returns the tempdir so the caller can keep it alive for the test.
fn spawn_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let global_db = GlobalDb::open(tmp.path().join("global.db")).unwrap();
    let multi_user = MultiUserStore::new(tmp.path().join("users"));
    let event_bus = EventBus::new();
    (
        AppState {
            multi_user,
            global_db,
            event_bus: event_bus.clone(),
            publisher: Arc::new(LocalEventPublisher::new(event_bus)),
            orchestrator: Arc::new(Orchestrator::from_sync_config(&SyncConfig::default()).unwrap()),
            etherscan: None,
            coingecko: policy_sync::CoinGeckoClient::new(),
            coordinator: Arc::new(policy_server::coordination::NoopCoordinator),
            sync_lock_ttl: std::time::Duration::from_secs(120),
        },
        tmp,
    )
}

async fn spawn_server(state: AppState) -> std::net::SocketAddr {
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    addr
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn postgres_backed_evaluate_persists_state_across_requests() {
    // 1. Build state, seed user store directly via the multi_user router.
    // Upsert the user FIRST so the wallet save satisfies `wallets_user_id_fkey`
    // (enforced by the Postgres backend). A unique email keeps the derived id
    // disjoint from other test files sharing the single integration DB. Reuse
    // the returned id for both the wallet store and the token so they match.
    let (state, _tmp) = spawn_state();
    let user_id = state
        .global_db
        .upsert_user("server-pg-evaluate@example.com", "test")
        .await
        .unwrap();
    let user_store = state.multi_user.for_user(&user_id).unwrap();

    let id = sample_wallet_id();
    let mut seeded = WalletState::new(id.clone());
    seeded.block_heights.insert(
        ChainId::ethereum_mainnet(),
        BlockHeight {
            number: 19_000_000,
            time: 1_700_000_000,
        },
    );
    user_store.save(&seeded).await.unwrap();

    // 2. Start the server.
    let token = mint_token(&user_id);
    let addr = spawn_server(state).await;

    // 3. POST /evaluate with the seeded user's token.
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/evaluate");
    let resp: EvaluateResponse = client
        .post(&url)
        .bearer_auth(&token)
        .json(&empty_envelope_request(id.clone()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.policy_request.state_before, seeded);
    assert_eq!(resp.policy_request.state_after, seeded);

    // 4. Same user, fresh store handle, should see the data persisted.
    let reloaded = user_store.load(&id).await.unwrap();
    assert_eq!(reloaded, seeded);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn health_endpoint_is_public() {
    let (state, _tmp) = spawn_state();
    let addr = spawn_server(state).await;
    let body = reqwest::get(format!("http://{addr}/health"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "ok");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn protected_route_rejects_missing_token() {
    let (state, _tmp) = spawn_state();
    let addr = spawn_server(state).await;
    let status = reqwest::get(format!("http://{addr}/wallets"))
        .await
        .unwrap()
        .status();
    assert_eq!(status, 401);
}
