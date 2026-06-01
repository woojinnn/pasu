//! axum application wiring — router, shared state, and HTTP adapters.
//! `/openapi.yaml`) sit outside the auth layer; everything else sits behind
//! `require_auth` middleware so a missing / invalid JWT is rejected before
//! the handler runs.
//! State is shared as a single `AppState` carrying the per-user DB router
//! (`MultiUserStore`) plus the cross-user identity DB (`GlobalDb`).

use axum::extract::{FromRef, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use std::sync::Arc;

use policy_db::{GlobalDb, MultiUserStore};
use policy_sync::{CoinGeckoClient, EtherscanClient, Orchestrator};

use crate::auth::{require_auth, AuthUser};
use crate::config::ServerConfig;
use crate::dashboard_handlers;
use crate::dto::EvaluateRequest;
use crate::events::{EventBus, EventPublisher};
use crate::handler::{evaluate, HandlerError};
use crate::read_handlers;
use crate::write_handlers;

/// Shared, cheaply-cloneable application state handed to every handler.
/// `multi_user` resolves one `PostgreSQL`-backed wallet store per authenticated
/// user. `global_db` is the cross-user identity DB (email ↔ `user_id`).
#[derive(Clone)]
pub struct AppState {
    pub multi_user: MultiUserStore,
    pub global_db: GlobalDb,
    pub event_bus: EventBus,
    /// Fanout boundary for server-originated events. The local publisher writes
    /// into `event_bus`; cloud deployments can replace it with Redis pub/sub.
    pub publisher: Arc<dyn EventPublisher>,
    /// Sync orchestrator — wraps the per-protocol fetchers wired from
    /// `scopeball-sync.toml`. Shared across handlers so we don't re-open
    /// HTTP connection pools on every request.
    pub orchestrator: Arc<Orchestrator>,
    /// Optional Etherscan V2 client — `None` when `ETHERSCAN_API_KEY`
    /// isn't set. `POST /wallets` uses it (when present) to discover
    /// every ERC-20 a wallet holds; absent it falls back to native-only.
    pub etherscan: Option<EtherscanClient>,
    /// `CoinGecko` metadata client — always present (free tier works
    /// keyless). `POST /wallets` calls it after discovery to backfill
    /// logo / website / description on newly-seen tokens. Lookups are
    /// best-effort; `CoinGecko` outages don't block wallet adds.
    pub coingecko: CoinGeckoClient,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Orchestrator / CoinGeckoClient aren't Debug.
        f.debug_struct("AppState")
            .field("multi_user", &self.multi_user)
            .field("global_db", &self.global_db)
            .field("event_bus", &self.event_bus)
            .field("publisher", &"<EventPublisher>")
            .field("orchestrator", &"<Orchestrator>")
            .field(
                "etherscan",
                &self.etherscan.as_ref().map(|_| "<EtherscanClient>"),
            )
            .field("coingecko", &"<CoinGeckoClient>")
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
/// - `GET  /openapi.yaml`                   — `OpenAPI` 3.0 spec.
/// - `GET  /auth/google`                    — redirect to Google consent.
/// - `GET  /auth/google/callback`           — finish OAuth → JWT.
///
/// Authenticated (`Authorization: Bearer <jwt>` OR `?token=<jwt>` on
/// SSE — see `auth::middleware` for the resolution order):
/// - `GET  /auth/me`                        — current user (id + email).
/// - `POST /evaluate`                       — simulate action envelope(s).
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
/// - `GET  /events/stream`                  — SSE live event feed.
///
/// Policy installation, policy catalogs, verdict history, audit views, and
/// finding feeds are intentionally extension-local. The cloud API only stores
/// wallet state, token metadata, transactions, and sync lifecycle data.
///
/// CORS is allowlist-based in cloud mode. Local defaults still allow the
/// dashboard development origins configured in [`ServerConfig`].
pub fn build_router(state: AppState) -> Router {
    let config = ServerConfig::from_env();
    build_router_with_config(state, &config)
}

/// Builds the service router with explicit runtime configuration.
pub fn build_router_with_config(state: AppState, config: &ServerConfig) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth_me_handler))
        .route("/evaluate", post(evaluate_handler))
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
        .route("/dashboard/summary", get(dashboard_handlers::get_summary))
        .route("/events/stream", get(crate::events::sse_stream))
        // Selector decode + revoke calldata builder + Cedar sequence sim
        // all moved to the dashboard (apps/web/src/tools/* + cedar/).
        // The server holds only wallet state and sync lifecycle data.
        .layer(from_fn(require_auth));

    let public = Router::new()
        .route("/health", get(health_handler))
        .route("/docs", get(crate::docs::docs_html))
        .route("/openapi.yaml", get(crate::docs::openapi_yaml))
        .route("/auth/google", get(crate::auth::start_google_login))
        .route("/auth/google/callback", get(crate::auth::google_callback))
        .route("/auth/refresh", post(crate::auth::refresh_token));

    public
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(config))
        .with_state(state)
}

fn cors_layer(config: &ServerConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .cors_allowed_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_private_network(config.allow_private_network)
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
