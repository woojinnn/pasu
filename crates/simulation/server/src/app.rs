//! axum application wiring — router, shared state, and HTTP adapters.
//!
//! Phase 5 split: public routes (`/auth/*`, `/health`) sit outside the auth
//! layer; everything else sits behind `require_auth` middleware so a missing
//! / invalid JWT is rejected before the handler runs.
//!
//! State is shared as a single `AppState` carrying both the user-DB router
//! (`MultiUserStore`) and the cross-user identity DB (`GlobalDb`). Handlers
//! pull whichever they need via axum extractors.

use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use tower_http::cors::CorsLayer;

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_sync::{EtherscanClient, Orchestrator};

use crate::auth::{require_auth, AuthUser};
use crate::dto::EvaluateRequest;
use crate::events::EventBus;
use crate::handler::{evaluate, HandlerError};
use crate::read_handlers;
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
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Orchestrator isn't Debug (large + irrelevant to formatting).
        f.debug_struct("AppState")
            .field("multi_user", &self.multi_user)
            .field("global_db", &self.global_db)
            .field("event_bus", &self.event_bus)
            .field("orchestrator", &"<Orchestrator>")
            .field(
                "etherscan",
                &self.etherscan.as_ref().map(|_| "<EtherscanClient>"),
            )
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
/// - `GET  /auth/google`                    — redirect to Google consent.
/// - `GET  /auth/google/callback`           — finish OAuth → JWT.
///
/// Authenticated (`Authorization: Bearer <jwt>` OR `?token=<jwt>` on
/// SSE — see `auth::middleware` for the resolution order):
/// - `GET  /auth/me`                        — current user (id + email).
/// - `POST /evaluate`                       — simulate action envelope(s).
/// - `GET  /wallets`                        — list user's wallets.
/// - `POST /wallets`                        — start tracking a new wallet.
/// - `POST /wallets/:address/sync`          — refresh via RPC/oracle.
/// - `GET  /wallets/:address/state`         — full wallet state.
/// - `GET  /wallets/:address/holdings`      — token holdings.
/// - `GET  /wallets/:address/approvals`     — approval set.
/// - `GET  /wallets/:address/block-heights` — per-chain sync block.
/// - `GET  /events/stream`                  — SSE live event feed.
///
/// CORS is `permissive` with private-network access enabled so both the
/// dashboard (127.0.0.1:5173) and the browser extension can reach the
/// server on 127.0.0.1.
pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth_me_handler))
        .route("/evaluate", post(evaluate_handler))
        .route(
            "/wallets",
            get(read_handlers::list_wallets).post(write_handlers::add_wallet),
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
        .route("/events/stream", get(crate::events::sse_stream))
        .layer(from_fn(require_auth));

    let public = Router::new()
        .route("/health", get(health_handler))
        .route("/auth/google", get(crate::auth::start_google_login))
        .route("/auth/google/callback", get(crate::auth::google_callback));

    public
        .merge(protected)
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
