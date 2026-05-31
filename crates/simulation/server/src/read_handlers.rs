//! Read-only handlers — the future web UI's window into the wallet DB.
//!
//! Every handler is auth-gated (Phase 5 `require_auth` middleware) and
//! receives an [`AuthUser`] via `Extension`. The user's `user_id` selects
//! the right `SqliteWalletStore` from [`MultiUserStore`]; handlers never
//! touch the DB directly.

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;

use simulation_db::MultiUserStore;
use simulation_state::approval::ApprovalSet;
use simulation_state::primitives::{Address, BlockHeight, ChainId};
use simulation_state::token::{TokenHolding, TokenKey};
use simulation_state::{WalletId, WalletState, WalletStore};

use simulation_db::repositories::{
    deltas, tokens as tokens_repo, user_policies, wallets as wallets_repo,
};

use crate::app::AppState;
use crate::auth::AuthUser;

/// `GET /wallets` — every wallet id the authenticated user has.
pub async fn list_wallets(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return open_store_error(&e.to_string()),
    };
    match store.list_wallets().await {
        Ok(ids) => Json(ids).into_response(),
        Err(e) => store_error(&e),
    }
}

/// `GET /wallets/:address/state` — the whole [`WalletState`].
///
/// Computed view fields (per-token `value_usd`, top-level
/// `portfolio_value_usd`) are populated here so the dashboard / UI can
/// render dollar values without re-computing balance × price.
pub async fn get_state(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
) -> Response {
    match load_state(&state.multi_user, &user.user_id, &address).await {
        Ok(mut s) => {
            s.populate_computed_values();
            Json(s).into_response()
        }
        Err(e) => e,
    }
}

/// `GET /wallets/:address/holdings` — token holdings as an array. Each
/// row includes the computed `value_usd`.
pub async fn get_holdings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
) -> Response {
    match load_state(&state.multi_user, &user.user_id, &address).await {
        Ok(mut s) => {
            s.populate_computed_values();
            #[derive(Serialize)]
            struct HoldingItem {
                key: TokenKey,
                #[serde(flatten)]
                holding: TokenHolding,
            }
            let items: Vec<HoldingItem> = s
                .tokens
                .into_iter()
                .map(|(key, holding)| HoldingItem { key, holding })
                .collect();
            Json(items).into_response()
        }
        Err(e) => e,
    }
}

/// `GET /wallets/:address/approvals` — the full [`ApprovalSet`].
pub async fn get_approvals(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
) -> Response {
    match load_state(&state.multi_user, &user.user_id, &address).await {
        Ok(s) => Json::<ApprovalSet>(s.approvals).into_response(),
        Err(e) => e,
    }
}

/// `GET /wallets/:address/block-heights` — per-chain block snapshot.
pub async fn get_block_heights(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
) -> Response {
    match load_state(&state.multi_user, &user.user_id, &address).await {
        Ok(s) => {
            #[derive(Serialize)]
            struct Item {
                chain: ChainId,
                #[serde(flatten)]
                height: BlockHeight,
            }
            let items: Vec<Item> = s
                .block_heights
                .into_iter()
                .map(|(chain, height)| Item { chain, height })
                .collect();
            Json(items).into_response()
        }
        Err(e) => e,
    }
}

/// Resolve `(user_id, address)` → load the full [`WalletState`]. Returns
/// an already-encoded HTTP error response on failure so callers can
/// pattern-match without trait noise.
async fn load_state(
    multi_user: &MultiUserStore,
    user_id: &str,
    address: &str,
) -> Result<WalletState, Response> {
    let addr = Address::from_str(address).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid address `{address}`: {e}"),
        )
            .into_response()
    })?;
    let store = multi_user
        .for_user(user_id)
        .map_err(|e| open_store_error(&e.to_string()))?;

    let known = store.list_wallets().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("list_wallets: {e}"),
        )
            .into_response()
    })?;
    let id = known
        .into_iter()
        .find(|w| w.address == addr)
        .unwrap_or_else(|| WalletId::new(addr, std::iter::empty::<ChainId>()));

    store
        .load(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("load: {e}")).into_response())
}

fn store_error(e: &simulation_state::store::StoreError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("store error: {e}"),
    )
        .into_response()
}

fn open_store_error(reason: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("open user store: {reason}"),
    )
        .into_response()
}

// ---------- /transactions ----------

/// One row in the response. Fields mirror `simulation_db::DeltaRow` but
/// JSON-shaped fields are deserialized so the client doesn't have to
/// double-parse. `realized_delta` is omitted when null.
#[derive(Serialize)]
struct TxRow {
    id: i64,
    source: String,
    status: String,
    created_at: i64,
    signed_at: Option<i64>,
    confirmed_at: Option<i64>,
    action_domain: String,
    action_kind: String,
    submitter: String,
    tx_hash: Option<String>,
    predicted_verdict: Option<String>,
    action: serde_json::Value,
    predicted_delta: Option<serde_json::Value>,
    realized_delta: Option<serde_json::Value>,
}

/// `GET /transactions?wallet=<address>&limit=<n>` — tx lifecycle log
/// from the `state_deltas` table for the authenticated user. When
/// `wallet` is omitted, returns deltas across every wallet in the
/// user's DB (up to `limit`, default 50, max 500).
pub async fn list_transactions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Query(query): axum::extract::Query<TxQuery>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return open_store_error(&e.to_string()),
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 500);

    // We need the wallet's i64 pk; the DeltaRow loader takes pk, not
    // address. Either filter by a specific wallet (when `wallet` is
    // set) or union across every wallet of the user.
    let pool = store.pool().clone();
    let wallet_filter = query.wallet.clone();
    let result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| {
            let walls = wallets_repo::list_active(tx)?;
            let candidates: Vec<i64> = match wallet_filter.as_deref() {
                Some(addr_filter) => {
                    let needle = addr_filter.to_lowercase();
                    walls
                        .into_iter()
                        .filter(|w| w.address == needle)
                        .map(|w| w.id)
                        .collect()
                }
                None => walls.into_iter().map(|w| w.id).collect(),
            };
            let mut out: Vec<TxRow> = Vec::new();
            for wid in candidates {
                let rows = deltas::list_recent(tx, wid, limit)?;
                for r in rows {
                    out.push(delta_row_to_dto(r));
                }
            }
            out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
            out.truncate(usize::try_from(limit).unwrap_or(50));
            Ok(out)
        })
    })
    .await;

    match result {
        Ok(Ok(rows)) => Json(rows).into_response(),
        Ok(Err(e)) => internal_str(&format!("list_transactions: {e}")),
        Err(e) => internal_str(&format!("join: {e}")),
    }
}

#[derive(serde::Deserialize)]
pub struct TxQuery {
    pub wallet: Option<String>,
    pub limit: Option<i64>,
}

fn delta_row_to_dto(r: simulation_db::repositories::DeltaRow) -> TxRow {
    TxRow {
        id: r.id,
        source: r.source,
        status: r.status,
        created_at: r.created_at,
        signed_at: r.signed_at,
        confirmed_at: r.confirmed_at,
        action_domain: r.action_domain,
        action_kind: r.action_kind,
        submitter: r.submitter,
        tx_hash: r.tx_hash,
        predicted_verdict: r.predicted_verdict,
        action: serde_json::from_str(&r.action_json).unwrap_or(serde_json::Value::Null),
        predicted_delta: r
            .predicted_delta_json
            .and_then(|s| serde_json::from_str(&s).ok()),
        realized_delta: r
            .realized_delta_json
            .and_then(|s| serde_json::from_str(&s).ok()),
    }
}

fn internal_str(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}

// ---------- /policies ----------

#[derive(Serialize)]
struct PolicyRow {
    id: i64,
    name: String,
    description: Option<String>,
    cedar_text: String,
    severity: String,
    enabled: bool,
    created_at: i64,
    updated_at: i64,
}

// ---------- /tokens (catalog) ----------

#[derive(Serialize)]
struct TokenCatalogRow {
    token_hash: String,
    key: TokenKey,
    symbol: Option<String>,
    decimals: Option<u8>,
    first_seen_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coingecko_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata_synced_at: Option<i64>,
}

/// `GET /tokens` — every token row in the user's catalog. Includes
/// CoinGecko-sourced metadata (logo / website / description) when
/// available.
pub async fn list_tokens(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return open_store_error(&e.to_string()),
    };
    let pool = store.pool().clone();
    let result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| {
            tokens_repo::list_all(tx).map(|rows| {
                rows.into_iter()
                    .map(|r| TokenCatalogRow {
                        token_hash: hex_token_hash(&r.token_hash),
                        key: r.key,
                        symbol: r.symbol,
                        decimals: r.decimals,
                        first_seen_at: r.first_seen_at,
                        logo_url: r.logo_url,
                        website_url: r.website_url,
                        description: r.description,
                        coingecko_id: r.coingecko_id,
                        metadata_synced_at: r.metadata_synced_at,
                    })
                    .collect::<Vec<_>>()
            })
        })
    })
    .await;
    match result {
        Ok(Ok(rows)) => Json(rows).into_response(),
        Ok(Err(e)) => internal_str(&format!("list_tokens: {e}")),
        Err(e) => internal_str(&format!("join: {e}")),
    }
}

fn hex_token_hash(h: &[u8; 16]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(2 + 32);
    out.push_str("0x");
    for b in h {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// `GET /policies` — every Cedar policy installed in the user's
/// `user_policies` table. Empty list for a fresh user.
pub async fn list_policies(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return open_store_error(&e.to_string()),
    };
    let pool = store.pool().clone();
    let result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| {
            user_policies::list_all(tx).map(|rows| {
                rows.into_iter()
                    .map(|r| PolicyRow {
                        id: r.id,
                        name: r.name,
                        description: r.description,
                        cedar_text: r.cedar_text,
                        severity: r.severity,
                        enabled: r.enabled,
                        created_at: r.created_at,
                        updated_at: r.updated_at,
                    })
                    .collect::<Vec<_>>()
            })
        })
    })
    .await;
    match result {
        Ok(Ok(rows)) => Json(rows).into_response(),
        Ok(Err(e)) => internal_str(&format!("list_policies: {e}")),
        Err(e) => internal_str(&format!("join: {e}")),
    }
}
