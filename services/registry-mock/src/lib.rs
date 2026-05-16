mod index;
mod manifest;
mod route_chains;
mod route_packages;
mod route_publish;
mod storage;

use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub storage: storage::Storage,
    pub index: Arc<index::Index>,
}

pub async fn build_app(state_dir: PathBuf) -> Router {
    tokio::fs::create_dir_all(&state_dir).await.ok();
    let storage = storage::Storage::new(state_dir);
    let index = Arc::new(index::Index::new());
    index.rebuild_from_disk(&storage).await.ok();
    let state = AppState { storage, index };
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/publish", post(route_publish::handle))
        .route("/packages/:name/:version/adapter.wasm", get(route_packages::wasm))
        .route("/packages/:name/:version/manifest.json", get(route_packages::manifest))
        .route("/chains/:chain/:address", get(route_chains::handle))
        .with_state(Arc::new(state))
}

pub async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let state_dir: PathBuf = std::env::var("REGISTRY_STATE")
        .unwrap_or_else(|_| "./state".into())
        .into();
    let app = build_app(state_dir).await;
    let addr: SocketAddr = std::env::var("REGISTRY_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    tracing::info!(%addr, "registry-mock listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
