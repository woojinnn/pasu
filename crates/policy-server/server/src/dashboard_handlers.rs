//! `GET /dashboard/summary` — Home page + Monitoring L1 single-call payload.
//!
//! Aggregates across every wallet the authenticated user tracks:
//! - total portfolio USD (sum of every holding's `value_usd`)
//! - per-chain USD breakdown (`{chain, usd, pct}`)
//! - per-wallet summary (`{address, label, total_usd, unlimited_count, pending_count}`)
//! - workspace-level counters (`wallet_count`)
//!
//! Implementation deliberately walks the user's primitive wallet snapshots
//! one-by-one and reuses `WalletState::populate_computed_values()`. Aggregate
//! read models are derived on request instead of persisted in the database.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;

use policy_state::primitives::ChainId;
use policy_state::{WalletId, WalletStore};

use crate::app::AppState;
use crate::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub wallet_count: usize,
    pub total_portfolio_usd: String,
    pub chain_breakdown: Vec<ChainShare>,
    pub wallets: Vec<WalletSummary>,
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

    let wallet_rows = match store.list_wallet_metadata().await {
        Ok(rows) => rows,
        Err(e) => return internal(&format!("dashboard wallets: {e}")),
    };

    // Walk each wallet's state in parallel (small N), compute per-wallet
    // total + per-chain split.
    let mut total: f64 = 0.0;
    let mut by_chain: std::collections::BTreeMap<ChainId, f64> = std::collections::BTreeMap::new();
    let mut wallet_summaries: Vec<WalletSummary> = Vec::with_capacity(wallet_rows.len());

    for (idx, w_row) in wallet_rows.into_iter().enumerate() {
        let wallet_id = match parse_addr(&w_row.address) {
            Some(addr) => WalletId::new(addr, w_row.chains.clone()),
            None => continue,
        };
        let Ok(mut s) = store.load(&wallet_id).await else {
            continue; // best-effort: skip on per-wallet load failure
        };
        let unlimited = count_unlimited_approvals(&s);
        let pending = i64::try_from(s.pending.len()).unwrap_or(i64::MAX);
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
            id: i64::try_from(idx).unwrap_or(i64::MAX),
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
        total_portfolio_usd: fmt6(total),
        chain_breakdown,
        wallets: wallet_summaries,
    })
    .into_response()
}

fn fmt6(v: f64) -> String {
    format!("{v:.6}")
}

fn parse_addr(addr_lower: &str) -> Option<policy_state::primitives::Address> {
    use std::str::FromStr;
    policy_state::primitives::Address::from_str(addr_lower).ok()
}

fn key_chain(k: &policy_state::token::TokenKey) -> &ChainId {
    k.chain()
}

fn count_unlimited_approvals(state: &policy_state::WalletState) -> i64 {
    let erc20 = state
        .approvals
        .erc20
        .values()
        .flat_map(std::collections::BTreeMap::values)
        .filter(|spec| spec.is_unlimited)
        .count();
    let set_for_all: usize = state
        .approvals
        .set_for_all
        .values()
        .map(std::collections::BTreeSet::len)
        .sum();
    i64::try_from(erc20 + set_for_all).unwrap_or(i64::MAX)
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}
