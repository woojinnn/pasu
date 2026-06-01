//! Read-only handlers — the future web UI's window into the wallet DB.
//!
//! Every handler is auth-gated (Phase 5 `require_auth` middleware) and
//! receives an [`AuthUser`] via `Extension`. The user's `user_id` selects
//! the right PostgreSQL wallet store from [`MultiUserStore`].

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

/// `GET /wallets/:address/approvals[?with_risk=true]`.
///
/// Default: returns the raw [`ApprovalSet`] (back-compat).
/// `?with_risk=true`: returns the classified shape — every approval gets
/// a `risk[]` tag list (UNLIMITED / OLD / EXPIRED). Spender labels +
/// KNOWN_VENUE/BLOCKED tags are no longer included on the server; that
/// data is operator-managed and lives in the (future) registry.
pub async fn get_approvals(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
    axum::extract::Query(q): axum::extract::Query<ApprovalsQuery>,
) -> Response {
    match load_state(&state.multi_user, &user.user_id, &address).await {
        Ok(s) => {
            if q.with_risk.unwrap_or(false) {
                let classified = classify_approvals(&s.approvals);
                Json(classified).into_response()
            } else {
                Json::<ApprovalSet>(s.approvals).into_response()
            }
        }
        Err(e) => e,
    }
}

#[derive(serde::Deserialize, Default)]
pub struct ApprovalsQuery {
    #[serde(default)]
    pub with_risk: Option<bool>,
}

/// Server-classified shape mirroring `ApprovalSet` but with risk tags.
#[derive(Serialize)]
struct ClassifiedApprovals {
    erc20: Vec<Erc20Approval>,
    set_for_all: Vec<SetForAllApproval>,
    permit2: Vec<Permit2Approval>,
}

#[derive(Serialize)]
struct Erc20Approval {
    chain: ChainId,
    token: String,
    spender: String,
    amount: String,
    is_unlimited: bool,
    last_set_at: i64,
    risk: Vec<&'static str>,
}

#[derive(Serialize)]
struct SetForAllApproval {
    chain: ChainId,
    collection: String,
    operator: String,
    risk: Vec<&'static str>,
}

#[derive(Serialize)]
struct Permit2Approval {
    chain: ChainId,
    token: String,
    spender: String,
    amount: String,
    expiration: i64,
    nonce: u32,
    risk: Vec<&'static str>,
}

/// 90-day staleness threshold — any approval older counts as `OLD`.
const STALE_AFTER_SECS: i64 = 90 * 24 * 3_600;

fn classify_approvals(set: &ApprovalSet) -> ClassifiedApprovals {
    use simulation_state::primitives::Time;

    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    )
    .unwrap_or(0);

    let _ = Time::from_unix(0); // silence import warning when only used via deref above

    let mut out = ClassifiedApprovals {
        erc20: Vec::new(),
        set_for_all: Vec::new(),
        permit2: Vec::new(),
    };

    for ((chain, token), per_spender) in &set.erc20 {
        for (spender, spec) in per_spender {
            let spender_lower = format!("{spender:#x}");
            let mut risk: Vec<&'static str> = Vec::new();
            if spec.is_unlimited {
                risk.push("UNLIMITED");
            }
            let last = i64::try_from(spec.last_set_at.as_unix()).unwrap_or(0);
            if last > 0 && now - last > STALE_AFTER_SECS {
                risk.push("OLD");
            }
            out.erc20.push(Erc20Approval {
                chain: chain.clone(),
                token: format!("{token:#x}"),
                spender: spender_lower,
                amount: spec.amount.to_string(),
                is_unlimited: spec.is_unlimited,
                last_set_at: last,
                risk,
            });
        }
    }

    for ((chain, collection), operators) in &set.set_for_all {
        for operator in operators {
            let operator_lower = format!("{operator:#x}");
            // setApprovalForAll always confers full collection access.
            let risk: Vec<&'static str> = vec!["UNLIMITED"];
            out.set_for_all.push(SetForAllApproval {
                chain: chain.clone(),
                collection: format!("{collection:#x}"),
                operator: operator_lower,
                risk,
            });
        }
    }

    for ((chain, token, spender), allowance) in &set.permit2 {
        let spender_lower = format!("{spender:#x}");
        let mut risk: Vec<&'static str> = Vec::new();
        if allowance.amount == simulation_state::primitives::U256::MAX {
            risk.push("UNLIMITED");
        }
        let exp = i64::try_from(allowance.expiration.as_unix()).unwrap_or(0);
        if exp > 0 && exp < now {
            risk.push("EXPIRED");
        }
        out.permit2.push(Permit2Approval {
            chain: chain.clone(),
            token: format!("{token:#x}"),
            spender: spender_lower,
            amount: allowance.amount.to_string(),
            expiration: exp,
            nonce: allowance.nonce,
            risk,
        });
    }
    out
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

/// `GET /transactions?wallet=<address>&limit=<n>` — transaction lifecycle log.
///
/// State deltas are no longer stored in the policy server's database. The
/// endpoint stays present for dashboard compatibility and returns an empty
/// collection until a dedicated lifecycle read model is reintroduced.
pub async fn list_transactions(
    State(_state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Query(query): axum::extract::Query<TxQuery>,
) -> Response {
    let _ = (&user.user_id, query.wallet, query.limit);
    Json(Vec::<TxRow>::new()).into_response()
}

#[derive(serde::Deserialize)]
pub struct TxQuery {
    pub wallet: Option<String>,
    pub limit: Option<i64>,
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

    let wallets = match store.list_wallets().await {
        Ok(wallets) => wallets,
        Err(e) => return store_error(&e),
    };
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for wallet in wallets {
        let state = match store.load(&wallet).await {
            Ok(state) => state,
            Err(e) => return store_error(&e),
        };
        for (key, holding) in state.tokens {
            let key_json = serde_json::to_string(&key).unwrap_or_else(|_| format!("{key:?}"));
            if !seen.insert(key_json.clone()) {
                continue;
            }
            let metadata = holding.metadata;
            rows.push(TokenCatalogRow {
                token_hash: key_json,
                key,
                symbol: Some(holding.symbol),
                decimals: Some(holding.decimals),
                first_seen_at: i64::try_from(holding.last_synced_at.as_unix()).unwrap_or(0),
                logo_url: metadata.as_ref().and_then(|m| m.logo_url.clone()),
                website_url: metadata.as_ref().and_then(|m| m.website_url.clone()),
                description: metadata.as_ref().and_then(|m| m.description.clone()),
                coingecko_id: metadata.and_then(|m| m.coingecko_id),
                metadata_synced_at: None,
            });
        }
    }

    Json(rows).into_response()
}
