//! `GET /dashboard/summary` — Home page + Monitoring L1 single-call payload.
//!
//! Aggregates across every wallet the authenticated user tracks:
//! - total portfolio USD (sum of every holding's `value_usd`)
//! - per-chain USD breakdown (`{chain, usd, pct}`)
//! - per-wallet summary (`{address, label, total_usd, unlimited_count, pending_count}`)
//! - workspace-level counters (wallet_count, policy_count, unresolved_findings)
//!
//! Implementation deliberately walks the user's wallets one-by-one and
//! reuses `WalletState::populate_computed_values()`. Typical users have
//! 1-5 wallets; a SQL-side aggregate would shave milliseconds but lose
//! the single source of truth (`Holding::compute_value_usd`).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;

use simulation_db::repositories::{
    approvals, pending_txs, user_policies, verdicts as verdicts_repo, wallets as wallets_repo,
};
use simulation_state::primitives::ChainId;
use simulation_state::{WalletId, WalletStore};

use crate::app::AppState;
use crate::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub wallet_count: usize,
    pub policy_count: usize,
    pub total_portfolio_usd: String,
    pub chain_breakdown: Vec<ChainShare>,
    pub wallets: Vec<WalletSummary>,
    pub unresolved_findings: i64,
}

#[derive(Debug, Serialize)]
pub struct ChainShare {
    pub chain: ChainId,
    pub usd: String,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub id: i64,
    pub address: String,
    pub label: Option<String>,
    pub total_usd: String,
    pub unlimited_count: i64,
    pub pending_count: i64,
}

pub async fn get_summary(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };

    // Pull the wallet list + per-wallet aggregate counts (unlimited
    // approvals, pending txs, policy count, unresolved findings) in one
    // spawn_blocking. Holdings/prices come from the async `WalletStore`
    // path below.
    let pool = store.pool().clone();
    let counts_result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| {
            let wallets = wallets_repo::list_active(tx)?;
            let policy_count = user_policies::list_all(tx)?.len();
            let mut per_wallet: Vec<(wallets_repo::Wallet, i64, i64)> =
                Vec::with_capacity(wallets.len());
            for w in &wallets {
                let unlimited = approvals::erc20::count_unlimited_for_wallet(tx, w.id)?;
                let pending = pending_txs::count_for_wallet(tx, w.id)?;
                per_wallet.push((w.clone(), unlimited, pending));
            }
            let unresolved = verdicts_repo::count_by_verdict(
                tx,
                &verdicts_repo::VerdictFilter {
                    verdict: Some("warn".into()),
                    limit: 1,
                    ..Default::default()
                },
            )?;
            // count_by_verdict aggregates pass/warn/fail; we only want
            // warns without a user decision. Run a tight follow-up query.
            let undecided_warns: i64 = tx.query_row(
                "SELECT COUNT(*) FROM verdicts WHERE verdict = 'warn' AND user_decision IS NULL",
                [],
                |r| r.get(0),
            )?;
            let _ = unresolved; // keep the binding for clarity above
            Ok((per_wallet, policy_count, undecided_warns))
        })
    })
    .await;
    let (per_wallet, policy_count, unresolved_findings) = match counts_result {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return internal(&format!("dashboard counts: {e}")),
        Err(e) => return internal(&format!("join: {e}")),
    };

    // Walk each wallet's state in parallel (small N), compute per-wallet
    // total + per-chain split.
    let mut total: f64 = 0.0;
    let mut by_chain: std::collections::BTreeMap<ChainId, f64> = std::collections::BTreeMap::new();
    let mut wallet_summaries: Vec<WalletSummary> = Vec::with_capacity(per_wallet.len());

    for (w_row, unlimited, pending) in per_wallet {
        let wallet_id = match parse_addr(&w_row.address) {
            Some(addr) => WalletId::new(addr, w_row.chains.iter().cloned()),
            None => continue,
        };
        let mut s = match store.load(&wallet_id).await {
            Ok(s) => s,
            Err(_) => continue, // best-effort: skip on per-wallet load failure
        };
        s.populate_computed_values();
        let wallet_total = s
            .portfolio_value_usd
            .as_ref()
            .and_then(|p| p.as_str().parse::<f64>().ok())
            .unwrap_or(0.0);
        total += wallet_total;

        // Per-chain breakdown: sum value_usd grouped by token.chain.
        for (key, holding) in &s.tokens {
            let chain = key_chain(key).clone();
            let v = holding
                .value_usd
                .as_ref()
                .and_then(|p| p.as_str().parse::<f64>().ok())
                .unwrap_or(0.0);
            if v > 0.0 {
                *by_chain.entry(chain).or_default() += v;
            }
        }

        wallet_summaries.push(WalletSummary {
            id: w_row.id,
            address: w_row.address,
            label: w_row.label,
            total_usd: fmt6(wallet_total),
            unlimited_count: unlimited,
            pending_count: pending,
        });
    }

    let chain_breakdown: Vec<ChainShare> = by_chain
        .into_iter()
        .map(|(chain, usd)| ChainShare {
            chain,
            usd: fmt6(usd),
            pct: if total > 0.0 {
                (usd / total) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    Json(DashboardSummary {
        wallet_count: wallet_summaries.len(),
        policy_count,
        total_portfolio_usd: fmt6(total),
        chain_breakdown,
        wallets: wallet_summaries,
        unresolved_findings,
    })
    .into_response()
}

fn fmt6(v: f64) -> String {
    format!("{v:.6}")
}

fn parse_addr(addr_lower: &str) -> Option<simulation_state::primitives::Address> {
    use std::str::FromStr;
    simulation_state::primitives::Address::from_str(addr_lower).ok()
}

fn key_chain(k: &simulation_state::token::TokenKey) -> &ChainId {
    k.chain()
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}
