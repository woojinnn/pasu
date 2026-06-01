//! Phase 5 — `/tx/decode`, `/approvals/revoke-plan`, `/simulate/sequence`.

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_server::app::{build_router, AppState};
use simulation_server::auth::jwt::{issue, TokenType};
use simulation_server::spenders::SpenderCatalog;
use simulation_sync::{Orchestrator, SyncConfig};

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";

fn mint_token(user_id: &str) -> String {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
    issue(user_id, "test@example.com", TokenType::Access, None).unwrap()
}

async fn spawn_server() -> (std::net::SocketAddr, tempfile::TempDir, String) {
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
        spenders: SpenderCatalog::empty(),
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (addr, tmp, mint_token("u_test_alice"))
}

#[tokio::test]
async fn decode_tx_recognizes_erc20_approve() {
    let (addr, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/tx/decode"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "chain": "eip155:1",
            "to": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "data": "0x095ea7b3000000000000000000000000111111111117dc0aa78b770fa6a738034120c302ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["selector"], "0x095ea7b3");
    assert_eq!(body["function_name"], "approve");
    assert_eq!(body["action_envelope"]["domain"], "token");
    assert_eq!(body["action_envelope"]["kind"], "erc20.approve");
}

#[tokio::test]
async fn decode_tx_returns_unknown_for_unrecognized_selector() {
    let (addr, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/tx/decode"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "chain": "eip155:1",
            "to": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "data": "0xdeadbeef00000000"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["selector"], "0xdeadbeef");
    assert!(body["function_name"].is_null());
    assert!(body["action_envelope"].is_null());
}

#[tokio::test]
async fn decode_tx_treats_empty_data_as_native_transfer() {
    let (addr, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/tx/decode"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "chain": "eip155:1",
            "to": "0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
            "data": "0x"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["action_envelope"]["domain"], "native");
    assert_eq!(body["action_envelope"]["kind"], "transfer");
}

#[tokio::test]
async fn revoke_plan_builds_approve_zero_calldata() {
    let (addr, _tmp, token) = spawn_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/approvals/revoke-plan"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "items": [
                {
                    "chain": "eip155:1",
                    "token": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                    "spender": "0x111111111117dc0aa78b770fa6a738034120c302",
                    "label": "USDC → 1inch"
                }
            ]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let call = &body["calls"][0];
    assert_eq!(call["to"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
    assert_eq!(call["value"], "0x0");
    assert_eq!(call["selector"], "0x095ea7b3");
    let data = call["data"].as_str().unwrap();
    // selector (4B) + spender (32B padded) + zero amount (32B)
    assert_eq!(data.len(), 2 + 8 + 64 + 64);
    assert!(data.starts_with("0x095ea7b3"));
    assert!(data.ends_with(&"0".repeat(64)));
    assert!(data.contains("111111111117dc0aa78b770fa6a738034120c302"));
}

#[tokio::test]
async fn revoke_plan_rejects_invalid_address() {
    let (addr, _tmp, token) = spawn_server().await;
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/approvals/revoke-plan"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "items": [
                { "chain": "eip155:1", "token": "0xnope", "spender": "0x111" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn simulate_sequence_passes_when_permit_all_policy_installed() {
    let (addr, _tmp, token) = spawn_server().await;
    // Install a wide-open permit policy.
    reqwest::Client::new()
        .post(format!("http://{addr}/policies"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "permit-all",
            "cedar_text": "permit(principal, action, resource);",
            "severity": "info"
        }))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/simulate/sequence"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "steps": [
                {
                    "label": "swap",
                    "principal": "Wallet::\"0xabc\"",
                    "action": "Action::\"swap\"",
                    "resource": "Protocol::\"0xdef\"",
                    "entities": [],
                    "context": {}
                },
                {
                    "label": "transfer",
                    "principal": "Wallet::\"0xabc\"",
                    "action": "Action::\"transfer\"",
                    "resource": "Protocol::\"0xfff\"",
                    "entities": [],
                    "context": {}
                }
            ]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["overall"], "pass");
    assert_eq!(body["steps"].as_array().unwrap().len(), 2);
    for step in body["steps"].as_array().unwrap() {
        assert_eq!(step["verdict"], "pass");
    }
}

#[tokio::test]
async fn simulate_sequence_fails_when_forbid_policy_matches() {
    let (addr, _tmp, token) = spawn_server().await;
    // Install a forbid policy with deny severity.
    reqwest::Client::new()
        .post(format!("http://{addr}/policies"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "forbid-all",
            "cedar_text": "forbid(principal, action, resource);",
            "severity": "deny"
        }))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/simulate/sequence"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "steps": [
                {
                    "label": "any",
                    "principal": "Wallet::\"0xabc\"",
                    "action": "Action::\"swap\"",
                    "resource": "Protocol::\"0xdef\"",
                    "entities": [],
                    "context": {}
                }
            ]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["overall"], "fail");
    assert_eq!(body["steps"][0]["verdict"], "fail");
    let outcomes = body["steps"][0]["policy_results"].as_array().unwrap();
    assert!(outcomes
        .iter()
        .any(|o| o["decision"] == "deny" && o["severity"] == "deny"));
}
