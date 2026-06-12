//! LIVE server-data-plane e2e for the HL `session-loss-circuit-breaker`
//! enrichment methods (`perp.equity_drawdown_bps` + `perp.session_fill_stats`).
//!
//! This is the **Procedure C** (real `/evaluate` data plane) proof for the
//! server-state package: a real axum server + real PostgreSQL persist a crafted
//! HL account (anchors + fill window) through the genuine `WalletStore` serde
//! path, then a real HTTP `/evaluate` request — keyed by the MASTER wallet, the
//! identity the SW prereq forwards — reads it back through the genuine handler
//! and returns the method outputs the Cedar atoms threshold on.
//!
//! What this closes beyond the in-process handler tests (`handler.rs`):
//!   * real PostgreSQL JSONB round-trip of the NEW state fields
//!     (`EquityAnchor`, `HlFillSummary`) — unit tests use `InMemoryWalletStore`;
//!   * real HTTP transport through `evaluate_handler` (auth → derive_user_id →
//!     load state by `wallet_id.address` → `execute_call_specs`).
//!
//! Run (Postgres on :5433, schema pre-applied):
//!   TEST_DATABASE_URL=postgres://scopeball:scopeball@127.0.0.1:5433/scopeball \
//!     cargo test -p policy-server --test perp_server_state_e2e -- --ignored --nocapture

use std::str::FromStr;
use std::sync::Arc;

use policy_db::{GlobalDb, MultiUserStore};
use policy_server::app::{build_router, AppState};
use policy_server::auth::jwt::{issue, TokenType};
use policy_server::events::{EventBus, LocalEventPublisher};
use policy_state::live_field::DataSource;
use policy_state::position::{EquityAnchor, HlAccount, HlFillSummary, Position, PositionKind};
use policy_state::primitives::{Address, ChainId, Decimal, Time};
use policy_state::{ProtocolRef, WalletId, WalletState, WalletStore};
use policy_sync::{Orchestrator, SyncConfig};
use serde_json::json;

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
    String,
) {
    let tmp = tempfile::tempdir().unwrap();
    let global_db = GlobalDb::open(tmp.path().join("global.db")).unwrap();
    // Unique email per spawn → distinct derived user_id (tests share one DB and
    // run on parallel threads; a shared email would race the `users` pkey).
    static USER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = USER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let user_id = global_db
        .upsert_user(&format!("perp-e2e-{n}@example.com"), "test")
        .await
        .unwrap();
    let multi_user = MultiUserStore::new(tmp.path().join("users"));
    let event_bus = EventBus::new();
    let state = AppState {
        multi_user: multi_user.clone(),
        global_db,
        event_bus: event_bus.clone(),
        publisher: Arc::new(LocalEventPublisher::new(event_bus)),
        orchestrator: Arc::new(Orchestrator::from_sync_config(&SyncConfig::default()).unwrap()),
        etherscan: None,
        coingecko: policy_sync::CoinGeckoClient::new(),
        coordinator: Arc::new(policy_server::coordination::NoopCoordinator),
        sync_lock_ttl: std::time::Duration::from_secs(120),
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let token = mint_token(&user_id);
    (addr, multi_user, tmp, token, user_id)
}

/// The MASTER wallet the venue order resolves to (the identity the SW prereq
/// forwards as `walletAddress` and the server loads state for).
const MASTER: &str = "0x1111111111111111111111111111111111111111";
/// A second wallet whose HL account has neither an equity baseline nor any
/// fills — both methods must skip it (fail-open / dormant), never fabricate.
const CONTROL: &str = "0x2222222222222222222222222222222222222222";

fn hl_position(account: HlAccount, synced_at: u64) -> Position {
    Position {
        id: "hyperliquid/account".into(),
        protocol: ProtocolRef::new("hyperliquid"),
        chain: None,
        kind: PositionKind::HyperliquidAccount(account),
        primitives_synced_at: Time::from_unix(synced_at),
        primitives_source: DataSource::VenueApi {
            endpoint: "https://api.hyperliquid.xyz/info".into(),
            parser_id: "hl_account".into(),
            auth: None,
        },
    }
}

/// Seed a crafted HL account that trips all four circuit-breaker atoms:
///   * equity 10000 → 9500  ⇒ dayDrawdownBps 500  (daily-loss-limit ≥ 500)
///   * HWM 10326.09 → 9500   ⇒ peakDrawdownBps 800  (max-drawdown ≥ 800)
///   * 3 most-recent closes negative ⇒ lossStreak 3 (loss-streak-cooldown ≥ 3)
///   * 16 fills today          ⇒ tradesToday 16     (overtrading > 15)
async fn seed_tripping(mu: &MultiUserStore, user_id: &str, now_secs: i64) {
    let store = mu.for_user(user_id).unwrap();
    let id = WalletId::new(
        Address::from_str(MASTER).unwrap(),
        [ChainId::ethereum_mainnet()],
    );
    let mut s = WalletState::new(id);

    let day_start_ms = (now_secs / 86_400) * 86_400 * 1000;
    let base = day_start_ms + 100_000;
    // Newest-first by realized PnL: [-10,-20,-30, +50, then 12×+5]. The +50 at
    // position 4 stops the streak at 3. All 16 land inside today's UTC window.
    // Σ closedPnl = -60 + 50 + 60 = 50.
    let pnls = [
        "-10", "-20", "-30", "50", "5", "5", "5", "5", "5", "5", "5", "5", "5", "5", "5", "5",
    ];
    let fills: Vec<HlFillSummary> = pnls
        .iter()
        .enumerate()
        .map(|(i, pnl)| HlFillSummary {
            tid: 1_000 + i as u64,
            // i=0 → largest time (newest); strictly descending, all > day_start.
            time: (base + (16 - i as i64) * 1_000) as u64,
            coin: "BTC".into(),
            closed_pnl: Decimal::new(*pnl),
            px: Decimal::new("100"),
            sz: Decimal::new("1"),
        })
        .collect();

    s.positions.push(hl_position(
        HlAccount {
            perp_account_value_usd: Some(Decimal::new("9500")),
            equity_baseline: Some(EquityAnchor {
                value: Decimal::new("10000"),
                anchored_at: Time::from_unix((now_secs - 40_000) as u64),
                trusted: true,
            }),
            equity_hwm: Some(Decimal::new("10326.09")),
            fill_window: fills,
            ..HlAccount::default()
        },
        now_secs as u64,
    ));
    store.save(&s).await.unwrap();
}

/// Seed a control HL account: present (so `find_hl_account` matches) but with no
/// baseline and an empty fill window — both methods return `None`.
async fn seed_control(mu: &MultiUserStore, user_id: &str, now_secs: i64) {
    let store = mu.for_user(user_id).unwrap();
    let id = WalletId::new(
        Address::from_str(CONTROL).unwrap(),
        [ChainId::ethereum_mainnet()],
    );
    let mut s = WalletState::new(id);
    s.positions.push(hl_position(
        HlAccount {
            perp_account_value_usd: Some(Decimal::new("1000")),
            // equity_baseline: None, equity_hwm: None, fill_window: empty
            ..HlAccount::default()
        },
        now_secs as u64,
    ));
    store.save(&s).await.unwrap();
}

/// Seed an account whose raw equity is down BUT the drop is entirely a capital
/// withdrawal (`cumulative_net_flow` −500): raw 9500, flow-neutral 9500−(−500) =
/// 10000 = the peak/baseline → drawdown must read 0. Exercises the Postgres
/// JSONB round-trip of `cumulative_net_flow` and the method's flow netting.
async fn seed_flow_neutral(mu: &MultiUserStore, user_id: &str, now_secs: i64) {
    let store = mu.for_user(user_id).unwrap();
    let id = WalletId::new(
        Address::from_str(MASTER).unwrap(),
        [ChainId::ethereum_mainnet()],
    );
    let mut s = WalletState::new(id);
    s.positions.push(hl_position(
        HlAccount {
            perp_account_value_usd: Some(Decimal::new("9500")),
            equity_baseline: Some(EquityAnchor {
                value: Decimal::new("10000"),
                anchored_at: Time::from_unix((now_secs - 40_000) as u64),
                trusted: true,
            }),
            equity_hwm: Some(Decimal::new("10000")),
            cumulative_net_flow: Decimal::new("-500"),
            ledger_cursor_ms: 1_781_000_000_000,
            ..HlAccount::default()
        },
        now_secs as u64,
    ));
    store.save(&s).await.unwrap();
}

/// Build the venue-shaped `/evaluate` request. The envelope body is irrelevant
/// to these methods (they read persisted state, not the body) — mirror a known
/// well-formed body and attach the two perp call_specs.
fn evaluate_req(address: &str, now_secs: i64) -> serde_json::Value {
    json!({
        "wallet_id": { "address": address, "chains": ["eip155:1"] },
        "envelopes": [{
            "meta": {
                "nature": {
                    "kind": "onchain_tx", "chain": "eip155:1", "nonce": 0,
                    "value": "0x0", "gas_limit": "0x0",
                    "gas_price": {
                        "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/eip155:1" },
                        "synced_at": now_secs, "value": "0x0"
                    }
                },
                "submitted_at": now_secs, "submitter": address
            },
            "body": {
                "domain": "staking", "action": "stake", "amount": "0xde0b6b3a7640000",
                "recipient": "0x00000000000000000000000000000000deadbeef",
                "venue": { "chain": "eip155:1", "name": "ethena_staked_usde", "vault": "0x9d39a5de30e57443bff2a8307a4256c8797a3497" }
            }
        }],
        "eval_context": { "chain": "eip155:1", "now": now_secs, "action_index": 0, "request_kind": "transaction", "simulation": "preview" },
        "call_specs": [
            { "manifest_id": "perp-daily-loss", "call_id": "dd::s", "method": "perp.equity_drawdown_bps", "params": { "chain_id": "hl-mainnet" }, "outputs": [], "optional": true },
            { "manifest_id": "perp-fills", "call_id": "fs::s", "method": "perp.session_fill_stats", "params": { "chain_id": "hl-mainnet", "now": now_secs }, "outputs": [], "optional": true }
        ]
    })
}

/// Same as [`evaluate_req`] but sets the `session_fill_stats` `min_loss_usd` band
/// param (a manifest literal) so the test can exercise per-policy threshold
/// configuration over the genuine HTTP + Postgres path.
fn evaluate_req_min_loss(address: &str, now_secs: i64, min_loss_usd: &str) -> serde_json::Value {
    let mut req = evaluate_req(address, now_secs);
    req["call_specs"][1]["params"]["min_loss_usd"] = json!(min_loss_usd);
    req
}

async fn post_evaluate(
    addr: std::net::SocketAddr,
    token: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(format!("http://{addr}/evaluate"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn perp_methods_served_over_http_from_postgres() {
    let (addr, mu, _tmp, token, user_id) = spawn_server().await;
    // Fixed UTC instant: day 20700, mid-day. Drives both the seed and the request.
    let now_secs: i64 = 20_700 * 86_400 + 50_000;
    seed_tripping(&mu, &user_id, now_secs).await;

    let resp = post_evaluate(addr, &token, evaluate_req(MASTER, now_secs)).await;
    let results = &resp["policyRequest"]["results"];
    eprintln!("LIVE /evaluate results = {results}");

    let dd = &results["dd::s"];
    assert_eq!(dd["dayDrawdownBps"], 500, "10000→9500 = 5% = 500bps");
    assert_eq!(
        dd["peakDrawdownBps"], 800,
        "HWM 10326.09→9500 = 8% = 800bps"
    );
    assert_eq!(dd["baselineTrusted"], true, "anchor labelled trusted");

    let fs = &results["fs::s"];
    assert_eq!(fs["lossStreak"], 3, "3 most-recent closes negative");
    assert_eq!(fs["lossesToday"], 3, "3 losing trades today (all >= $1)");
    assert_eq!(
        fs["tradesToday"], 16,
        "all 16 fills inside today's UTC window"
    );
    assert_eq!(fs["realizedPnlTodayUsd"], 50, "Σ -60 + 50 + 60 = 50");

    // The $1 band flows as a real manifest literal param: at min_loss_usd = 25
    // only the -$30 close is "meaningful", so the streak and loss-count collapse
    // to 1 — proving param → method over the genuine HTTP + Postgres path.
    let banded = post_evaluate(addr, &token, evaluate_req_min_loss(MASTER, now_secs, "25")).await;
    let bfs = &banded["policyRequest"]["results"]["fs::s"];
    assert_eq!(bfs["lossStreak"], 1, "at $25 band only -30 is meaningful");
    assert_eq!(bfs["lossesToday"], 1, "at $25 band only -30 counts");
    assert_eq!(bfs["tradesToday"], 16, "band does not affect frequency");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn perp_methods_absent_without_anchors_or_fills() {
    let (addr, mu, _tmp, token, user_id) = spawn_server().await;
    let now_secs: i64 = 20_700 * 86_400 + 50_000;
    seed_control(&mu, &user_id, now_secs).await;

    let resp = post_evaluate(addr, &token, evaluate_req(CONTROL, now_secs)).await;
    let results = &resp["policyRequest"]["results"];
    eprintln!(
        "CONTROL /evaluate results = {results}, diagnostics = {}",
        resp["diagnostics"]
    );

    // No baseline / empty window → methods return None → call_ids absent from the
    // results map (fail-open: the Cedar `context has <field>` guard stays dormant).
    assert!(
        results.get("dd::s").is_none(),
        "no baseline → drawdown absent"
    );
    assert!(results.get("fs::s").is_none(), "empty fills → stats absent");

    // …and each skip is surfaced as a top-level diagnostic naming the method.
    let diags = resp["diagnostics"].as_array().cloned().unwrap_or_default();
    let joined = diags
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<String>();
    assert!(
        joined.contains("perp.equity_drawdown_bps"),
        "diag names drawdown: {joined}"
    );
    assert!(
        joined.contains("perp.session_fill_stats"),
        "diag names fills: {joined}"
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn perp_drawdown_is_flow_neutral_over_http() {
    let (addr, mu, _tmp, token, user_id) = spawn_server().await;
    let now_secs: i64 = 20_700 * 86_400 + 50_000;
    seed_flow_neutral(&mu, &user_id, now_secs).await;

    let resp = post_evaluate(addr, &token, evaluate_req(MASTER, now_secs)).await;
    let dd = &resp["policyRequest"]["results"]["dd::s"];
    eprintln!("FLOW-NEUTRAL /evaluate dd = {dd}");
    // Raw equity 9500 vs a 10000 peak would read 500 bps — but the −500 is a
    // pure withdrawal, so the flow-neutral drawdown is 0 end-to-end (proves the
    // Postgres JSONB round-trip of cumulative_net_flow + the method's netting).
    assert_eq!(
        dd["dayDrawdownBps"], 0,
        "withdrawal must not read as a daily loss"
    );
    assert_eq!(
        dd["peakDrawdownBps"], 0,
        "withdrawal must not read as drawdown"
    );
}
