//! Phase 2 verdict endpoints — POST /verdicts, GET /audit/verdicts,
//! /audit/counts, /audit/export, /history/verdicts, /findings/feed, and
//! PATCH /verdicts/:id.

use std::str::FromStr;
use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_state::primitives::{Address, ChainId};
use simulation_state::{WalletId, WalletState};
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
        spenders: simulation_server::spenders::SpenderCatalog::empty(),
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

/// Seed an empty wallet so the verdicts endpoint can resolve the address
/// to a `wallets.id` PK.
async fn seed_wallet(multi_user: &MultiUserStore, user_id: &str, addr: Address) {
    use simulation_state::WalletStore;
    let store = multi_user.for_user(user_id).unwrap();
    let id = WalletId::new(addr, [ChainId::ethereum_mainnet()]);
    store.save(&WalletState::new(id)).await.unwrap();
}

fn sample_body(wallet: &str, verdict: &str, origin: &str) -> serde_json::Value {
    serde_json::json!({
        "wallet": wallet,
        "verdict": verdict,
        "severity": if verdict == "fail" { "deny" } else { "warn" },
        "dapp_origin": origin,
        "method": "eth_sendTransaction",
        "decoded_fn": "swapExactTokensForTokens",
        "contract": { "addr": "0xe592427a0aece92de3edee1f18e0157c05861564", "symbol": "UniRouter" },
        "selector": { "sig": "0x38ed1739", "decoded": "swapExact…" },
        "policy_name": "Max slippage 0.5%",
        "reason": { "ko": "슬리피지 초과", "en": "Slippage exceeds 0.5%" }
    })
}

const WALLET_ADDR: &str = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045";

async fn boot_with_wallet() -> (std::net::SocketAddr, String) {
    let (addr, mu, _tmp, token) = spawn_server().await;
    let wallet = Address::from_str(WALLET_ADDR).unwrap();
    seed_wallet(&mu, "u_test_alice", wallet).await;
    // tempdir kept alive by leaking — fine for tests
    std::mem::forget(_tmp);
    (addr, token)
}

#[tokio::test]
async fn post_verdict_then_audit_returns_row() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();

    let post = client
        .post(format!("http://{addr}/verdicts"))
        .bearer_auth(&token)
        .json(&sample_body(WALLET_ADDR, "warn", "app.uniswap.org"))
        .send()
        .await
        .unwrap();
    assert_eq!(post.status(), 200, "POST /verdicts failed");
    let created: serde_json::Value = post.json().await.unwrap();
    assert!(created["id"].as_i64().is_some());

    let list: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/audit/verdicts"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    let row = &list[0];
    assert_eq!(row["verdict"], "warn");
    assert_eq!(row["dapp_origin"], "app.uniswap.org");
    assert_eq!(row["reason"]["ko"], "슬리피지 초과");
    assert_eq!(row["reason"]["en"], "Slippage exceeds 0.5%");
    assert_eq!(row["contract"]["symbol"], "UniRouter");
    assert_eq!(row["wallet"], WALLET_ADDR);
}

#[tokio::test]
async fn audit_filter_by_verdict_and_origin() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    for (v, o) in [
        ("pass", "uniswap.org"),
        ("warn", "uniswap.org"),
        ("fail", "opensea.io"),
    ] {
        client
            .post(format!("http://{addr}/verdicts"))
            .bearer_auth(&token)
            .json(&sample_body(WALLET_ADDR, v, o))
            .send()
            .await
            .unwrap();
    }
    let list: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/audit/verdicts?verdict=fail"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["dapp_origin"], "opensea.io");

    let list: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/audit/verdicts?origin=uniswap.org"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn audit_counts_aggregates() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    for v in ["pass", "pass", "warn", "fail"] {
        client
            .post(format!("http://{addr}/verdicts"))
            .bearer_auth(&token)
            .json(&sample_body(WALLET_ADDR, v, "a.com"))
            .send()
            .await
            .unwrap();
    }
    let body: serde_json::Value = client
        .get(format!("http://{addr}/audit/counts"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["pass"], 2);
    assert_eq!(body["warn"], 1);
    assert_eq!(body["fail"], 1);
}

#[tokio::test]
async fn audit_export_csv_header_and_row() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    client
        .post(format!("http://{addr}/verdicts"))
        .bearer_auth(&token)
        .json(&sample_body(WALLET_ADDR, "fail", "app.uniswap.org"))
        .send()
        .await
        .unwrap();
    let resp = client
        .get(format!("http://{addr}/audit/export"))
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
    assert!(ct.starts_with("text/csv"));
    let text = resp.text().await.unwrap();
    assert!(text.starts_with("id,ts,wallet,verdict,severity"));
    assert!(text.contains("fail"));
    assert!(text.contains("app.uniswap.org"));
}

#[tokio::test]
async fn patch_verdict_records_decision() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/verdicts"))
        .bearer_auth(&token)
        .json(&sample_body(WALLET_ADDR, "warn", "a.com"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_i64().unwrap();

    let patch = client
        .patch(format!("http://{addr}/verdicts/{id}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "decision": "trusted" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 204);

    let list: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/audit/verdicts"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list[0]["user_decision"], "trusted");
    assert!(list[0]["decided_at"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn patch_verdict_rejects_bad_decision() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("http://{addr}/verdicts/1"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "decision": "ignored" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn post_verdict_for_untracked_wallet_returns_404() {
    let (addr, _mu, _tmp, token) = spawn_server().await;
    // No wallet seeded.
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/verdicts"))
        .bearer_auth(&token)
        .json(&sample_body(WALLET_ADDR, "pass", "a.com"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn audit_filter_by_unknown_wallet_returns_empty() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    client
        .post(format!("http://{addr}/verdicts"))
        .bearer_auth(&token)
        .json(&sample_body(WALLET_ADDR, "warn", "a.com"))
        .send()
        .await
        .unwrap();
    let list: Vec<serde_json::Value> = client
        .get(format!(
            "http://{addr}/audit/verdicts?wallet=0x0000000000000000000000000000000000000000"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn history_cursor_paginates() {
    let (addr, token) = boot_with_wallet().await;
    let client = reqwest::Client::new();
    let mut ids = Vec::new();
    for _ in 0..5 {
        let created: serde_json::Value = client
            .post(format!("http://{addr}/verdicts"))
            .bearer_auth(&token)
            .json(&sample_body(WALLET_ADDR, "pass", "a.com"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        ids.push(created["id"].as_i64().unwrap());
    }

    let page1: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/history/verdicts?limit=2"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0]["id"], ids[4]);

    let cursor = page1.last().unwrap()["id"].as_i64().unwrap();
    let page2: Vec<serde_json::Value> = client
        .get(format!(
            "http://{addr}/history/verdicts?limit=2&before={cursor}"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);
    assert!(page2[0]["id"].as_i64().unwrap() < cursor);
}
