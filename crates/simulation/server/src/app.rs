//! axum application wiring — router, shared state, and HTTP adapters.
//!
//! Phase 5 split: public routes (`/auth/*`, `/health`, `/docs`,
//! `/openapi.yaml`) sit outside the auth layer; everything else sits behind
//! `require_auth` middleware so a missing / invalid JWT is rejected before
//! the handler runs.
//!
//! State is shared as a single `AppState` carrying the per-user DB router
//! (`MultiUserStore`) plus the cross-user identity DB (`GlobalDb`). Execution
//! reports are persisted into each user's own SQLite via
//! `SqliteExecutionReportStore` constructed on demand from the user's pool.

use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_sync::{CoinGeckoClient, EtherscanClient, Orchestrator};

use crate::auth::{require_auth, AuthUser};
use crate::db_store::SqliteExecutionReportStore;
use crate::dto::{EvaluateRequest, ExecutionReportRequest};
use crate::events::EventBus;
use crate::handler::{evaluate, report_execution, HandlerError};
use crate::cedar_handlers;
use crate::dashboard_handlers;
use crate::read_handlers;
use crate::verdict_handlers;
use crate::write_handlers;

/// Shared, cheaply-cloneable application state handed to every handler.
///
/// `multi_user` opens (and caches) one SQLite store per authenticated user.
/// `global_db` is the single cross-user identity DB (email ↔ user_id).
#[derive(Clone)]
pub struct AppState {
    pub multi_user: MultiUserStore,
    pub global_db: GlobalDb,
    pub event_bus: EventBus,
    /// Sync orchestrator — wraps the per-protocol fetchers wired from
    /// `scopeball-sync.toml`. Shared across handlers so we don't re-open
    /// HTTP connection pools on every request.
    pub orchestrator: Arc<Orchestrator>,
    /// Optional Etherscan V2 client — `None` when `ETHERSCAN_API_KEY`
    /// isn't set. `POST /wallets` uses it (when present) to discover
    /// every ERC-20 a wallet holds; absent it falls back to native-only.
    pub etherscan: Option<EtherscanClient>,
    /// CoinGecko metadata client — always present (free tier works
    /// keyless). `POST /wallets` calls it after discovery to backfill
    /// logo / website / description on newly-seen tokens. Lookups are
    /// best-effort; CoinGecko outages don't block wallet adds.
    pub coingecko: CoinGeckoClient,
    /// Spender reputation catalog — loaded once at startup from
    /// `scopeball-spenders.toml`. Drives `GET /spenders/:addr` and the
    /// approval risk classifier.
    pub spenders: crate::spenders::SpenderCatalog,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Orchestrator / CoinGeckoClient aren't Debug.
        f.debug_struct("AppState")
            .field("multi_user", &self.multi_user)
            .field("global_db", &self.global_db)
            .field("event_bus", &self.event_bus)
            .field("orchestrator", &"<Orchestrator>")
            .field(
                "etherscan",
                &self.etherscan.as_ref().map(|_| "<EtherscanClient>"),
            )
            .field("coingecko", &"<CoinGeckoClient>")
            .field("spenders", &format_args!("<{} entries>", self.spenders.len()))
            .finish()
    }
}

// Sub-state extractors so handlers can ask for just the piece they need.
impl FromRef<AppState> for MultiUserStore {
    fn from_ref(s: &AppState) -> Self {
        s.multi_user.clone()
    }
}

impl FromRef<AppState> for GlobalDb {
    fn from_ref(s: &AppState) -> Self {
        s.global_db.clone()
    }
}

impl FromRef<AppState> for EventBus {
    fn from_ref(s: &AppState) -> Self {
        s.event_bus.clone()
    }
}

impl FromRef<AppState> for Arc<Orchestrator> {
    fn from_ref(s: &AppState) -> Self {
        s.orchestrator.clone()
    }
}

/// Builds the service router.
///
/// Public (no auth):
/// - `GET  /health`                         — liveness probe.
/// - `GET  /docs`                           — Swagger UI page.
/// - `GET  /openapi.yaml`                   — OpenAPI 3.0 spec.
/// - `GET  /auth/google`                    — redirect to Google consent.
/// - `GET  /auth/google/callback`           — finish OAuth → JWT.
///
/// Authenticated (`Authorization: Bearer <jwt>` OR `?token=<jwt>` on
/// SSE — see `auth::middleware` for the resolution order):
/// - `GET  /auth/me`                        — current user (id + email).
/// - `POST /evaluate`                       — simulate action envelope(s).
/// - `POST /execution-report`               — post-policy lifecycle facts.
/// - `GET  /wallets`                        — list user's wallets.
/// - `POST /wallets`                        — start tracking a new wallet.
/// - `PATCH/DELETE /wallets/:address`       — label/owned + archive.
/// - `POST /wallets/:address/sync`          — refresh via RPC/oracle.
/// - `GET  /wallets/:address/state`         — full wallet state.
/// - `GET  /wallets/:address/holdings`      — token holdings.
/// - `GET  /wallets/:address/approvals`     — approval set.
/// - `GET  /wallets/:address/block-heights` — per-chain sync block.
/// - `GET  /transactions`                   — state-delta lifecycle log.
/// - `GET  /tokens`                         — token catalog + metadata.
/// - `GET/POST /policies`                   — Cedar policies CRUD.
/// - `PATCH/DELETE /policies/:id`           — single-policy update / delete.
/// - `GET  /events/stream`                  — SSE live event feed.
///
/// CORS is `permissive` with private-network access enabled so both the
/// dashboard (127.0.0.1:5173) and the browser extension can reach the
/// server on 127.0.0.1.
pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth_me_handler))
        .route("/evaluate", post(evaluate_handler))
        .route("/execution-report", post(execution_report_handler))
        .route(
            "/wallets",
            get(read_handlers::list_wallets).post(write_handlers::add_wallet),
        )
        .route(
            "/wallets/:address",
            axum::routing::patch(write_handlers::patch_wallet)
                .delete(write_handlers::delete_wallet),
        )
        .route("/wallets/:address/sync", post(write_handlers::sync_wallet))
        .route("/wallets/:address/state", get(read_handlers::get_state))
        .route(
            "/wallets/:address/holdings",
            get(read_handlers::get_holdings),
        )
        .route(
            "/wallets/:address/approvals",
            get(read_handlers::get_approvals),
        )
        .route(
            "/wallets/:address/block-heights",
            get(read_handlers::get_block_heights),
        )
        .route("/transactions", get(read_handlers::list_transactions))
        .route("/tokens", get(read_handlers::list_tokens))
        .route(
            "/policies",
            get(read_handlers::list_policies).post(write_handlers::create_policy),
        )
        .route(
            "/policies/:id",
            get(read_handlers::get_policy)
                .patch(write_handlers::patch_policy)
                .delete(write_handlers::delete_policy),
        )
        // ---- Phase 4: cedar editor support ----
        .route(
            "/policies/validate",
            post(cedar_handlers::validate_policy),
        )
        .route(
            "/policies/:id/test",
            post(cedar_handlers::test_policy),
        )
        .route("/policy-schema", get(read_handlers::get_policy_schema))
        .route("/policy-templates", get(read_handlers::get_policy_templates))
        .route(
            "/examples/transactions",
            get(read_handlers::get_example_transactions),
        )
        .route("/spenders/:addr", get(read_handlers::get_spender))
        // ---- Phase 3: dashboard summary ----
        .route("/dashboard/summary", get(dashboard_handlers::get_summary))
        // ---- Phase 2: verdict / audit / history / findings ----
        .route("/verdicts", post(verdict_handlers::create_verdict))
        .route(
            "/verdicts/:id",
            axum::routing::patch(verdict_handlers::patch_verdict),
        )
        .route("/audit/verdicts", get(verdict_handlers::list_audit))
        .route("/audit/counts", get(verdict_handlers::audit_counts))
        .route("/audit/export", get(verdict_handlers::audit_export))
        .route("/history/verdicts", get(verdict_handlers::list_history))
        .route("/findings/feed", get(verdict_handlers::findings_feed))
        .route("/events/stream", get(crate::events::sse_stream))
        .layer(from_fn(require_auth));

    let public = Router::new()
        .route("/health", get(health_handler))
        .route("/docs", get(crate::docs::docs_html))
        .route("/openapi.yaml", get(crate::docs::openapi_yaml))
        .route("/auth/google", get(crate::auth::start_google_login))
        .route("/auth/google/callback", get(crate::auth::google_callback));

    public
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive().allow_private_network(true))
        .with_state(state)
}

/// `GET /health` — liveness probe.
async fn health_handler() -> &'static str {
    "ok"
}

/// `GET /auth/me` — echo the authenticated user. Used by the dashboard
/// to validate a stored JWT on page load and render the profile chip.
async fn auth_me_handler(Extension(user): Extension<AuthUser>) -> Response {
    Json(serde_json::json!({
        "user_id": user.user_id,
        "email": user.email,
    }))
    .into_response()
}

/// `POST /evaluate` — JSON in, JSON out. Requires auth (Phase 5).
///
/// Maps [`HandlerError::Reducer`] to `422 Unprocessable Entity` (the action is
/// invalid for the state) and [`HandlerError::Store`] to `500 Internal Server
/// Error` (persistence failed).
async fn evaluate_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<EvaluateRequest>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("open user store: {e}"),
            )
                .into_response();
        }
    };
    match evaluate(&*store, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(err @ HandlerError::Reducer(_)) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string()).into_response()
        }
        Err(err @ HandlerError::Store(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    }
}

/// `POST /execution-report` — record what happened after policy approval.
///
/// Reports land in the authenticated user's own SQLite so wallet/chain/venue
/// callbacks stay isolated per user. Canonical wallet state still comes from
/// the sync orchestrator; this endpoint only persists lifecycle facts that a
/// later reconciler will confirm against chain receipts or venue snapshots.
async fn execution_report_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<ExecutionReportRequest>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("open user store: {e}"),
            )
                .into_response();
        }
    };
    let report_store = SqliteExecutionReportStore::new(store.pool().clone());
    match report_execution(&report_store, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}
