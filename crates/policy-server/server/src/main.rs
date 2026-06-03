//! `policy-server` binary entry point.
//!
//! Starts the axum HTTP service: initializes tracing, connects to PostgreSQL,
//! prepares the per-user store router, wires the sync orchestrator
//! (RPC/oracle/venue fetchers from `scopeball-sync.toml`),
//! and serves on `POLICY_SERVER_ADDR` (default `127.0.0.1:8788`).
//!
//! Environment variables:
//! - `POLICY_SERVER_ADDR` — bind address (default `127.0.0.1:8788`).
//! - `DATABASE_URL` — PostgreSQL connection URL (required).
//! - `SCOPEBALL_SYNC_CONFIG` — path to the sync TOML (default
//!   `./scopeball-sync.toml`). Required for any RPC/price fetching.
//! - `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `GOOGLE_REDIRECT_URI`,
//!   `JWT_SECRET`, `DASHBOARD_URL` — auth config (see `.env.example`).
//!
//! Periodic sync is handled by the standalone `sync_worker` binary. The API
//! process also supports on-demand sync via `POST /wallets/:addr/sync`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tracing_subscriber::EnvFilter;

use policy_server::app::{build_router_with_config, AppState, ShutdownRx};
use policy_server::config::ServerConfig;
use policy_server::coordination::build_coordinator;
use policy_server::events::{
    spawn_redis_event_forwarder, EventBus, EventPublisher, LocalEventPublisher, RedisEventPublisher,
};
use policy_server::storage::StorageBackend;
use policy_sync::{CoinGeckoClient, EtherscanClient, Orchestrator, SyncConfig};

/// Default sync config path. Lives next to the workspace root so the dev
/// loop is one command (`cargo run -p policy-server`).
const DEFAULT_SYNC_CONFIG: &str = "./scopeball-sync.toml";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Walks up from CWD to find `.env`. Silent if missing — production
    // deployments inject env vars directly.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,policy_server=debug")),
        )
        .init();

    let config = ServerConfig::from_env();
    tracing::info!("opening PostgreSQL policy-server storage");
    let storage = StorageBackend::open(&config).await?;
    let coordinator = build_coordinator(&config).await?;

    // Sync orchestrator. Load the TOML config; if the file is missing we
    // boot with an empty config (no RPC providers) — endpoints that
    // require sync will return 503-ish errors instead of crashing the
    // whole server at startup, so a dev can run /auth/* alone.
    let sync_config_path = std::env::var("SCOPEBALL_SYNC_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SYNC_CONFIG));
    let sync_config = match SyncConfig::load_file(&sync_config_path) {
        Ok(cfg) => {
            tracing::info!(
                path = %sync_config_path.display(),
                "loaded sync config"
            );
            cfg
        }
        Err(e) => {
            tracing::warn!(
                path = %sync_config_path.display(),
                error = %e,
                "sync config not loaded — sync endpoints will fail until fixed"
            );
            SyncConfig::default()
        }
    };
    let orchestrator = Arc::new(Orchestrator::from_sync_config(&sync_config)?);

    let etherscan = EtherscanClient::from_env();
    if etherscan.is_some() {
        tracing::info!("Etherscan token discovery enabled");
    } else {
        tracing::info!(
            "ETHERSCAN_API_KEY not set — POST /wallets will discover the native gas balance only"
        );
    }

    let coingecko = CoinGeckoClient::from_env();
    tracing::info!("CoinGecko token metadata client ready");

    let event_bus = EventBus::new();
    let publisher: Arc<dyn EventPublisher> = match config.redis_url.as_deref() {
        Some(url) if !url.trim().is_empty() => {
            let channel = config.redis_events_channel.clone();
            let _forwarder =
                spawn_redis_event_forwarder(url, channel.clone(), event_bus.clone()).await?;
            tracing::info!(channel, "Redis event fanout enabled");
            Arc::new(RedisEventPublisher::connect(url, config.redis_events_channel.clone()).await?)
        }
        _ => Arc::new(LocalEventPublisher::new(event_bus.clone())),
    };
    let state = AppState {
        multi_user: storage.multi_user(),
        global_db: storage.global_db(),
        event_bus: event_bus.clone(),
        publisher,
        orchestrator,
        etherscan,
        coingecko,
        coordinator,
        sync_lock_ttl: Duration::from_secs(config.sync_lock_ttl_secs),
    };
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let router = build_router_with_config(state, &config)
        .layer(axum::Extension(ShutdownRx(shutdown_rx)));

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "policy-server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;
    Ok(())
}

/// Resolves when the process receives SIGTERM (k8s) or Ctrl-C (local), then
/// broadcasts shutdown so SSE streams drain before the server stops accepting.
async fn shutdown_signal(tx: tokio::sync::watch::Sender<bool>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received; draining SSE streams");
    let _ = tx.send(true);
}
