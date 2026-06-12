//! LIVE seed+serve harness for the BROWSER A-plane e2e (NOT a committable test —
//! a long-running scaffold). Boots a real axum policy-server on a FIXED port
//! (8788) backed by the real Postgres (`TEST_DATABASE_URL`), seeds many HL
//! master wallets (one per A-plane server-state scenario, incl. threshold
//! boundaries) under ONE deterministic signed-in user, prints the minted dev
//! JWT, then BLOCKS (serves forever) so the built extension can drive real venue
//! orders through `fetch-hook → SW → /evaluate → WASM → verdict`.
//!
//! The venue order's `vaultAddress` selects which seeded wallet (scenario) the
//! server-state methods read. Each address isolates ONE atom at its boundary so
//! the per-policy e2e can assert FIRE (>= / > / <= threshold) and PASS (just on
//! the safe side) independently.
//!
//! Run (Postgres on :5433):
//!   TEST_DATABASE_URL=postgres://scopeball:scopeball@127.0.0.1:5433/scopeball \
//!     cargo test -p policy-server --test serve_seeded_browser_e2e \
//!     serve_seeded_for_browser -- --ignored --nocapture

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

const TEST_SECRET: &str = "test-secret-only-do-not-use-in-production-2026-05-31";
const EMAIL: &str = "perp-browser-e2e@scopeball.dev";

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

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

async fn save_account(
    mu: &MultiUserStore,
    user_id: &str,
    addr: &str,
    account: HlAccount,
    now: i64,
) {
    let store = mu.for_user(user_id).unwrap();
    let id = WalletId::new(
        Address::from_str(addr).unwrap(),
        [ChainId::ethereum_mainnet()],
    );
    let mut s = WalletState::new(id);
    s.positions.push(hl_position(account, now as u64));
    store.save(&s).await.unwrap();
}

/// Equity anchors only (no fills) — for the two drawdown atoms.
fn acct_equity(value: &str, baseline: &str, hwm: &str, now: i64) -> HlAccount {
    HlAccount {
        perp_account_value_usd: Some(Decimal::new(value)),
        equity_baseline: Some(EquityAnchor {
            value: Decimal::new(baseline),
            anchored_at: Time::from_unix((now - 40_000) as u64),
            trusted: true,
        }),
        equity_hwm: Some(Decimal::new(hwm)),
        ..HlAccount::default()
    }
}

/// Fill window (newest-first pnl list) at flat equity — for the four fill-stats
/// atoms. `pnls[0]` is the newest trade.
fn acct_fills(pnls: &[&str], now: i64) -> HlAccount {
    let day_start_ms = (now / 86_400) * 86_400 * 1000;
    let base = day_start_ms + 100_000;
    let n = pnls.len() as i64;
    let fills: Vec<HlFillSummary> = pnls
        .iter()
        .enumerate()
        .map(|(i, pnl)| HlFillSummary {
            tid: 1_000 + i as u64,
            // i=0 → largest time (newest); strictly descending, all inside today.
            time: (base + (n - i as i64) * 1_000) as u64,
            coin: "BTC".into(),
            closed_pnl: Decimal::new(*pnl),
            px: Decimal::new("100"),
            sz: Decimal::new("1"),
        })
        .collect();
    HlAccount {
        // Flat equity at the 10000 peak ⇒ drawdown atoms read 0 (isolates fills).
        perp_account_value_usd: Some(Decimal::new("10000")),
        equity_baseline: Some(EquityAnchor {
            value: Decimal::new("10000"),
            anchored_at: Time::from_unix((now - 40_000) as u64),
            trusted: true,
        }),
        equity_hwm: Some(Decimal::new("10000")),
        fill_window: fills,
        ..HlAccount::default()
    }
}

/// (label, 40-hex address, builder) — one wallet per A-plane scenario. Addresses
/// are mirrored into the case matrix the driver fires.
fn scenarios(now: i64) -> Vec<(&'static str, &'static str, HlAccount)> {
    vec![
        // ── legacy multi-atom wallets (kept; used by the first-pass e2e) ──
        // TRIP4: day 500 / peak 800 / streak 3 / trades 16.
        ("TRIP4", "0x1111111111111111111111111111111111111111", {
            let mut a = acct_fills(
                &[
                    "-10", "-20", "-30", "50", "5", "5", "5", "5", "5", "5", "5", "5", "5", "5",
                    "5", "5",
                ],
                now,
            );
            a.perp_account_value_usd = Some(Decimal::new("9500"));
            a.equity_hwm = Some(Decimal::new("10326.09"));
            a
        }),
        (
            "CONTROL",
            "0x2222222222222222222222222222222222222222",
            HlAccount {
                perp_account_value_usd: Some(Decimal::new("1000")),
                ..HlAccount::default()
            },
        ),
        // ── boundary wallets (one atom each) ──
        // daily-loss-limit: dayDrawdownBps >= 500.
        (
            "DLIM_FIRE",
            "0xe2e0000000000000000000000000000000000001",
            acct_equity("9500", "10000", "10000", now),
        ),
        (
            "DLIM_PASS",
            "0xe2e0000000000000000000000000000000000002",
            acct_equity("9501", "10000", "10000", now),
        ),
        // max-drawdown: peakDrawdownBps >= 800 (baseline low so day stays 0 → isolates peak).
        (
            "MDD_FIRE",
            "0xe2e0000000000000000000000000000000000003",
            acct_equity("9200", "9200", "10000", now),
        ),
        (
            "MDD_PASS",
            "0xe2e0000000000000000000000000000000000004",
            acct_equity("9201", "9201", "10000", now),
        ),
        // loss-streak: lossStreak >= 3.
        (
            "STREAK_FIRE",
            "0xe2e0000000000000000000000000000000000005",
            acct_fills(&["-10", "-20", "-30"], now),
        ),
        (
            "STREAK_PASS",
            "0xe2e0000000000000000000000000000000000006",
            acct_fills(&["-10", "-20", "50"], now),
        ),
        // overtrading: tradesToday > 15 (all small wins → no loss atoms).
        (
            "OVER_FIRE",
            "0xe2e0000000000000000000000000000000000007",
            acct_fills(&["5"; 16], now),
        ),
        (
            "OVER_PASS",
            "0xe2e0000000000000000000000000000000000008",
            acct_fills(&["5"; 15], now),
        ),
        // daily-loss-count: lossesToday >= 5 (interspersed wins → streak stays 1, isolates count).
        (
            "COUNT_FIRE",
            "0xe2e0000000000000000000000000000000000009",
            acct_fills(
                &["-10", "5", "-10", "5", "-10", "5", "-10", "5", "-10"],
                now,
            ),
        ),
        (
            "COUNT_PASS",
            "0xe2e000000000000000000000000000000000000a",
            acct_fills(&["-10", "5", "-10", "5", "-10", "5", "-10"], now),
        ),
        // daily-realized-loss: realizedPnlTodayUsd <= -500 (one trade → streak 1, count 1, isolates realized).
        (
            "REAL_FIRE",
            "0xe2e000000000000000000000000000000000000b",
            acct_fills(&["-500"], now),
        ),
        (
            "REAL_PASS",
            "0xe2e000000000000000000000000000000000000c",
            acct_fills(&["-499"], now),
        ),
    ]
}

#[tokio::test]
#[ignore = "long-running browser-e2e seed+serve scaffold; requires TEST_DATABASE_URL"]
async fn serve_seeded_for_browser() {
    // CI runs `--ignored` tests; this one blocks forever (serves on 8788), so it
    // must NO-OP unless explicitly invoked for a local browser session. Without
    // the opt-in env var it returns immediately — keeping the CI ignored-suite
    // (and `--test-threads=1` serialization) from hanging.
    if std::env::var("BROWSER_E2E_SERVE").is_err() {
        eprintln!("serve_seeded_for_browser: set BROWSER_E2E_SERVE=1 to run (no-op in CI)");
        return;
    }

    std::env::set_var("JWT_SECRET", TEST_SECRET);
    let now = now_unix();

    let tmp = tempfile::tempdir().unwrap();
    let global_db = GlobalDb::open(tmp.path().join("global.db")).unwrap();
    let user_id = global_db.upsert_user(EMAIL, "test").await.unwrap();
    let multi_user = MultiUserStore::new(tmp.path().join("users"));

    let scen = scenarios(now);
    for (_label, addr, account) in &scen {
        save_account(&multi_user, &user_id, addr, account.clone(), now).await;
    }

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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8788")
        .await
        .unwrap();
    let token = issue(&user_id, EMAIL, TokenType::Access, None).unwrap();

    let mut addr_lines = String::new();
    for (label, addr, _) in &scen {
        addr_lines.push_str(&format!("{label}={addr}\n"));
    }
    let summary =
        format!("READY\nuser_id={user_id}\nemail={EMAIL}\nnow={now}\n{addr_lines}JWT={token}\n");
    std::fs::write("/tmp/perp-browser-e2e.txt", &summary).unwrap();
    eprintln!("==== SEED+SERVE READY (127.0.0.1:8788) ====\n{summary}");

    axum::serve(listener, router).await.unwrap();
}
