//! axum application wiring — router, shared state, and HTTP adapters.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;

use simulation_sync::WalletStore;

use crate::dto::{EvaluateRequest, ExecutionReportRequest};
use crate::handler::{evaluate, report_execution, HandlerError};
use crate::store::ExecutionReportStore;

/// Shared, cheaply-cloneable application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    /// The wallet-state persistence boundary. `InMemoryWalletStore` in dev/test;
    /// the DB owner's `SQLite` [`WalletStore`] in production.
    pub store: Arc<dyn WalletStore>,
    /// The execution-report persistence boundary. Kept separate from
    /// [`WalletStore`] because report events are lifecycle metadata, not
    /// authoritative wallet state.
    pub execution_reports: Arc<dyn ExecutionReportStore>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `dyn WalletStore` is not `Debug`; describe the shape instead.
        f.debug_struct("AppState")
            .field("store", &"Arc<dyn WalletStore>")
            .field("execution_reports", &"Arc<dyn ExecutionReportStore>")
            .finish()
    }
}

/// Builds the service router.
///
/// Routes:
/// - `POST /evaluate` — simulate actions over wallet state.
/// - `POST /execution-report` — record post-policy wallet/chain/venue outcome.
/// - `GET  /health`   — liveness probe, returns `"ok"`.
///
/// CORS is `permissive` with private-network access enabled so the browser
/// extension can reach the server on `127.0.0.1`.
//
// `Router` is itself `#[must_use]`, so no attribute is added here.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/evaluate", post(evaluate_handler))
        .route("/execution-report", post(execution_report_handler))
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive().allow_private_network(true))
        .with_state(state)
}

/// `GET /health` — liveness probe.
async fn health_handler() -> &'static str {
    "ok"
}

/// `POST /evaluate` — JSON in, JSON out.
///
/// Maps [`HandlerError::Reducer`] to `422 Unprocessable Entity` (the action is
/// invalid for the state) and [`HandlerError::Store`] to `500 Internal Server
/// Error` (persistence failed).
async fn evaluate_handler(
    State(state): State<AppState>,
    Json(req): Json<EvaluateRequest>,
) -> Response {
    match evaluate(&*state.store, req).await {
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
/// This endpoint accepts both wallet-mediated results (signature / tx hash) and
/// direct venue results such as Hyperliquid `/exchange` acceptance. It records
/// the lifecycle event only; canonical wallet state still comes from sync.
async fn execution_report_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecutionReportRequest>,
) -> Response {
    match report_execution(&*state.execution_reports, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}
