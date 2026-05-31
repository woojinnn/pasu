//! Verdict endpoints — Audit / History / Findings + write-back from
//! the extension after Cedar evaluation.
//!
//! Endpoints (all auth-gated):
//! - `POST   /verdicts`          — extension submits a verdict after eval
//! - `GET    /audit/verdicts`    — filtered list (range / verdict / origin / search)
//! - `GET    /audit/counts`      — pass/warn/fail summary under same filter
//! - `GET    /audit/export`      — CSV export of the filtered list
//! - `GET    /history/verdicts`  — cursor pagination (before_id)
//! - `GET    /findings/feed`     — recent stream (since id, default newest 50)
//! - `PATCH  /verdicts/:id`      — user resolves a `warn` (trusted/cancelled)
//!
//! See `crates/simulation/db/src/repositories/verdicts.rs` for the underlying
//! query API. Address resolution: any endpoint that takes a wallet address
//! resolves it to the per-user `wallets.id` PK via the deltas-style
//! `list_active` walk; misses return an empty result rather than 404 so the
//! FE can render an "empty state" panel without special-casing.

use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use simulation_db::repositories::{verdicts, wallets as wallets_repo};
use simulation_state::primitives::Address;

use crate::app::AppState;
use crate::auth::AuthUser;
use crate::events::types::{Event, FindingEvent};

// ---------- POST /verdicts ----------

#[derive(Debug, Deserialize)]
pub struct CreateVerdictReq {
    /// Wallet the action was attempted from. Required.
    pub wallet: String,
    /// Outcome of the cedar evaluation.
    pub verdict: String, // pass | warn | fail
    /// Severity of the matched policy.
    pub severity: String, // deny | warn | info
    #[serde(default)]
    pub delta_id: Option<i64>,
    #[serde(default)]
    pub policy_id: Option<i64>,
    #[serde(default)]
    pub dapp_origin: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub decoded_fn: Option<String>,
    #[serde(default)]
    pub contract: Option<ContractRef>,
    #[serde(default)]
    pub selector: Option<SelectorRef>,
    #[serde(default)]
    pub policy_name: Option<String>,
    #[serde(default)]
    pub reason: Option<I18nReason>,
}

#[derive(Debug, Deserialize)]
pub struct ContractRef {
    pub addr: String,
    #[serde(default)]
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SelectorRef {
    pub sig: String,
    #[serde(default)]
    pub decoded: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct I18nReason {
    #[serde(default)]
    pub ko: Option<String>,
    #[serde(default)]
    pub en: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateVerdictResp {
    pub id: i64,
    pub ts: i64,
}

pub async fn create_verdict(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateVerdictReq>,
) -> Response {
    if !matches!(req.verdict.as_str(), "pass" | "warn" | "fail") {
        return bad_request("verdict must be one of: pass | warn | fail");
    }
    if !matches!(req.severity.as_str(), "deny" | "warn" | "info") {
        return bad_request("severity must be one of: deny | warn | info");
    }
    let addr = match Address::from_str(&req.wallet) {
        Ok(a) => a,
        Err(e) => return bad_request(&format!("invalid wallet `{}`: {e}", req.wallet)),
    };
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let now = unix_now();
    let addr_lower = format!("{addr:#x}");

    let insert = verdicts::VerdictInsert {
        delta_id: req.delta_id,
        wallet_id: 0,
        policy_id: req.policy_id,
        severity: req.severity.clone(),
        verdict: req.verdict.clone(),
        ts: now,
        dapp_origin: req.dapp_origin.clone(),
        method: req.method.clone(),
        decoded_fn: req.decoded_fn.clone(),
        contract_addr: req.contract.as_ref().map(|c| c.addr.clone()),
        contract_symbol: req.contract.as_ref().and_then(|c| c.symbol.clone()),
        selector_sig: req.selector.as_ref().map(|s| s.sig.clone()),
        selector_decoded: req.selector.as_ref().and_then(|s| s.decoded.clone()),
        policy_name: req.policy_name.clone(),
        reason_ko: req.reason.as_ref().and_then(|r| r.ko.clone()),
        reason_en: req.reason.as_ref().and_then(|r| r.en.clone()),
    };

    let result = tokio::task::spawn_blocking({
        let mut insert = insert;
        move || {
            pool.with_tx(|tx| {
                let Some(w) = wallets_repo::get_by_address(tx, &addr_lower)? else {
                    return Ok(None);
                };
                insert.wallet_id = w.id;
                let id = verdicts::insert(tx, &insert)?;
                Ok(Some(id))
            })
        }
    })
    .await;

    let id = match result {
        Ok(Ok(Some(id))) => id,
        Ok(Ok(None)) => return not_found("wallet not tracked for this user"),
        Ok(Err(e)) => return internal(&format!("create_verdict: {e}")),
        Err(e) => return internal(&format!("join: {e}")),
    };

    // Fan out a SSE finding event so monitoring pages refresh in real time.
    state.event_bus.publish(
        user.user_id.clone(),
        Event::Finding(FindingEvent {
            id,
            ts: now,
            wallet: format!("{addr:#x}"),
            verdict: req.verdict,
            severity: req.severity,
            dapp_origin: req.dapp_origin,
            policy_name: req.policy_name,
        }),
    );

    Json(CreateVerdictResp { id, ts: now }).into_response()
}

// ---------- GET /audit/verdicts + /history/verdicts + /findings/feed ----------

/// Query parameters shared by /audit, /history, /findings.
#[derive(Debug, Deserialize, Default)]
pub struct VerdictListQuery {
    /// Time range alias: "1h" | "6h" | "24h" | "7d". Computed against `now`.
    /// When set, overrides `since` / `until`.
    pub range: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub verdict: Option<String>,
    pub origin: Option<String>,
    pub policy_id: Option<i64>,
    pub wallet: Option<String>,
    pub search: Option<String>,
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct VerdictDto {
    pub id: i64,
    pub ts: i64,
    pub wallet: Option<String>,
    pub verdict: String,
    pub severity: String,
    pub method: Option<String>,
    pub decoded_fn: Option<String>,
    pub dapp_origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<ContractDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<SelectorDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    pub reason: I18nReason,
    pub user_decision: Option<String>,
    pub decided_at: Option<i64>,
    pub delta_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ContractDto {
    pub addr: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SelectorDto {
    pub sig: String,
    pub decoded: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PolicyRef {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub severity: String,
}

fn row_to_dto(
    r: verdicts::VerdictRow,
    wallet_addr: Option<String>,
) -> VerdictDto {
    let contract = r.contract_addr.as_ref().map(|addr| ContractDto {
        addr: addr.clone(),
        symbol: r.contract_symbol.clone(),
    });
    let selector = r.selector_sig.as_ref().map(|sig| SelectorDto {
        sig: sig.clone(),
        decoded: r.selector_decoded.clone(),
    });
    let policy = if r.policy_id.is_some() || r.policy_name.is_some() {
        Some(PolicyRef {
            id: r.policy_id,
            name: r.policy_name.clone(),
            severity: r.severity.clone(),
        })
    } else {
        None
    };
    VerdictDto {
        id: r.id,
        ts: r.ts,
        wallet: wallet_addr,
        verdict: r.verdict,
        severity: r.severity,
        method: r.method,
        decoded_fn: r.decoded_fn,
        dapp_origin: r.dapp_origin,
        contract,
        selector,
        policy,
        reason: I18nReason {
            ko: r.reason_ko,
            en: r.reason_en,
        },
        user_decision: r.user_decision,
        decided_at: r.decided_at,
        delta_id: r.delta_id,
    }
}

/// Resolve query → repository filter. Returns the filter + (when set) the
/// optional wallet PK lookup so the caller can pre-validate.
async fn resolve_filter(
    q: &VerdictListQuery,
    state: &AppState,
    user_id: &str,
) -> Result<(verdicts::VerdictFilter, std::collections::HashMap<i64, String>), Response> {
    let store = state
        .multi_user
        .for_user(user_id)
        .map_err(|e| internal(&format!("open user store: {e}")))?;
    let pool = store.pool().clone();

    let now = unix_now();
    let (since_ts, until_ts) = match q.range.as_deref() {
        Some(r) => {
            let secs = match r {
                "1h" => 3_600,
                "6h" => 6 * 3_600,
                "24h" => 24 * 3_600,
                "7d" => 7 * 24 * 3_600,
                other => return Err(bad_request(&format!(
                    "range `{other}` not one of: 1h | 6h | 24h | 7d"
                ))),
            };
            (Some(now - secs), Some(now))
        }
        None => (q.since, q.until),
    };

    // Resolve `wallet=0x…` query → wallet_id PK (so deletions / address
    // re-use are handled correctly).
    let wallet_filter_addr = q.wallet.clone();
    let wallets_index = tokio::task::spawn_blocking(move || {
        pool.with_tx(wallets_repo::list_active)
    })
    .await
    .map_err(|e| internal(&format!("join: {e}")))?
    .map_err(|e| internal(&format!("list_active: {e}")))?;

    let mut by_id: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut wallet_id_filter: Option<i64> = None;
    for w in &wallets_index {
        by_id.insert(w.id, w.address.clone());
    }
    if let Some(addr_q) = wallet_filter_addr {
        let needle = addr_q.to_lowercase();
        wallet_id_filter = wallets_index.iter().find(|w| w.address == needle).map(|w| w.id);
        if wallet_id_filter.is_none() {
            // Caller filtered by an address the user doesn't own → empty
            // result. Express via an impossible filter so the SQL still
            // runs but returns 0 rows.
            wallet_id_filter = Some(-1);
        }
    }

    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    Ok((
        verdicts::VerdictFilter {
            since_ts,
            until_ts,
            verdict: q.verdict.clone(),
            origin: q.origin.clone(),
            policy_id: q.policy_id,
            wallet_id: wallet_id_filter,
            search: q.search.clone(),
            before_id: q.before,
            limit,
        },
        by_id,
    ))
}

pub async fn list_audit(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<VerdictListQuery>,
) -> Response {
    let (filter, by_id) = match resolve_filter(&q, &state, &user.user_id).await {
        Ok(t) => t,
        Err(e) => return e,
    };
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let rows = match tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| verdicts::list_filtered(tx, &filter))
    })
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return internal(&format!("list: {e}")),
        Err(e) => return internal(&format!("join: {e}")),
    };
    let dtos: Vec<VerdictDto> = rows
        .into_iter()
        .map(|r| {
            let wallet = by_id.get(&r.wallet_id).cloned();
            row_to_dto(r, wallet)
        })
        .collect();
    Json(dtos).into_response()
}

pub async fn list_history(
    state: State<AppState>,
    user: Extension<AuthUser>,
    query: Query<VerdictListQuery>,
) -> Response {
    // History is the same shape as audit; the FE just renders it differently
    // (grouping, sequence emphasis). Identical filter semantics.
    list_audit(state, user, query).await
}

pub async fn findings_feed(
    state: State<AppState>,
    user: Extension<AuthUser>,
    mut query: Query<VerdictListQuery>,
) -> Response {
    // Default to "everything new" if the FE didn't pin a cursor.
    if query.limit.is_none() {
        query.limit = Some(50);
    }
    list_audit(state, user, query).await
}

#[derive(Debug, Serialize)]
pub struct VerdictCountsDto {
    pub pass: i64,
    pub warn: i64,
    pub fail: i64,
}

pub async fn audit_counts(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<VerdictListQuery>,
) -> Response {
    let (filter, _) = match resolve_filter(&q, &state, &user.user_id).await {
        Ok(t) => t,
        Err(e) => return e,
    };
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let counts = match tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| verdicts::count_by_verdict(tx, &filter))
    })
    .await
    {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => return internal(&format!("counts: {e}")),
        Err(e) => return internal(&format!("join: {e}")),
    };
    Json(VerdictCountsDto {
        pass: counts.pass,
        warn: counts.warn,
        fail: counts.fail,
    })
    .into_response()
}

pub async fn audit_export(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<VerdictListQuery>,
) -> Response {
    let (filter, by_id) = match resolve_filter(&q, &state, &user.user_id).await {
        Ok(t) => t,
        Err(e) => return e,
    };
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let rows = match tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| verdicts::list_filtered(tx, &filter))
    })
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return internal(&format!("export: {e}")),
        Err(e) => return internal(&format!("join: {e}")),
    };

    let mut csv = String::from(
        "id,ts,wallet,verdict,severity,dapp_origin,method,decoded_fn,contract_addr,policy_name,reason_en,user_decision\n"
    );
    for r in rows {
        let wallet = by_id.get(&r.wallet_id).cloned().unwrap_or_default();
        let line = format!(
            "{id},{ts},{wallet},{verdict},{severity},{origin},{method},{fn_name},{contract},{policy},{reason},{decision}\n",
            id = r.id,
            ts = r.ts,
            wallet = csv_escape(&wallet),
            verdict = r.verdict,
            severity = r.severity,
            origin = csv_escape(r.dapp_origin.as_deref().unwrap_or("")),
            method = csv_escape(r.method.as_deref().unwrap_or("")),
            fn_name = csv_escape(r.decoded_fn.as_deref().unwrap_or("")),
            contract = csv_escape(r.contract_addr.as_deref().unwrap_or("")),
            policy = csv_escape(r.policy_name.as_deref().unwrap_or("")),
            reason = csv_escape(r.reason_en.as_deref().unwrap_or("")),
            decision = csv_escape(r.user_decision.as_deref().unwrap_or("")),
        );
        csv.push_str(&line);
    }

    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "text/csv; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
            (CONTENT_DISPOSITION, "attachment; filename=\"verdicts.csv\""),
        ],
        csv,
    )
        .into_response()
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_owned()
    }
}

// ---------- PATCH /verdicts/:id ----------

#[derive(Debug, Deserialize)]
pub struct PatchVerdictReq {
    /// "trusted" | "cancelled".
    pub decision: String,
}

pub async fn patch_verdict(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<PatchVerdictReq>,
) -> Response {
    if !matches!(req.decision.as_str(), "trusted" | "cancelled") {
        return bad_request("decision must be one of: trusted | cancelled");
    }
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let now = unix_now();
    let decision = req.decision;
    let result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| verdicts::set_decision(tx, id, &decision, now))
    })
    .await;
    match result {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => not_found("verdict not found"),
        Ok(Err(e)) => internal(&format!("patch_verdict: {e}")),
        Err(e) => internal(&format!("join: {e}")),
    }
}

// ---------- helpers ----------

fn bad_request(reason: &str) -> Response {
    (StatusCode::BAD_REQUEST, reason.to_owned()).into_response()
}

fn not_found(reason: &str) -> Response {
    (StatusCode::NOT_FOUND, reason.to_owned()).into_response()
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}
