//! `simulation-server` binary entry point.
//!
//! Starts the axum HTTP service: initializes tracing, opens the cross-user
//! identity DB (`~/.scopeball/global.db`), prepares the per-user store
//! router (`~/.scopeball/users/<id>/scopeball.db`), wires the sync
//! orchestrator (RPC/oracle/venue fetchers from `scopeball-sync.toml`),
//! and serves on `SIMULATION_SERVER_ADDR` (default `127.0.0.1:8788`).
//!
//! Environment variables:
//! - `SIMULATION_SERVER_ADDR` — bind address (default `127.0.0.1:8788`).
//! - `SCOPEBALL_HOME` — overrides `~/.scopeball` (test / sandboxing).
//! - `SCOPEBALL_SYNC_CONFIG` — path to the sync TOML (default
//!   `./scopeball-sync.toml`). Required for any RPC/price fetching.
//! - `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `GOOGLE_REDIRECT_URI`,
//!   `JWT_SECRET`, `DASHBOARD_URL` — auth config (see `.env.example`).
//!
//! The background `Scheduler` from `simulation-sync` is not wired here yet —
//! sync runs on-demand via `POST /wallets/:addr/sync`. A multi-user-aware
//! periodic ticker that walks every user's wallets is follow-up work.

use std::path::PathBuf;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use simulation_server::app::{build_router_with_config, AppState};
use simulation_server::config::ServerConfig;
use simulation_server::events::EventBus;
use simulation_server::storage::StorageBackend;
use simulation_sync::{CoinGeckoClient, EtherscanClient, Orchestrator, SyncConfig};

/// Default sync config path. Lives next to the workspace root so the dev
/// loop is one command (`cargo run -p simulation-server`).
const DEFAULT_SYNC_CONFIG: &str = "./scopeball-sync.toml";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Walks up from CWD to find `.env`. Silent if missing — production
    // deployments inject env vars directly.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,simulation_server=debug")),
        )
        .init();

    let config = ServerConfig::from_env();
    let home = scopeball_home();
    let global_db_path = home.join("global.db");
    let users_dir = home.join("users");
    tracing::info!(
        global_db = %global_db_path.display(),
        users_dir = %users_dir.display(),
        "opening multi-user wallet store"
    );

    let storage = StorageBackend::open(&config, &home)?;

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

    let state = AppState {
        multi_user: storage.multi_user(),
        global_db: storage.global_db(),
        event_bus: EventBus::new(),
        orchestrator,
        etherscan,
        coingecko,
    };
    let router = build_router_with_config(state, config.clone());

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "simulation-server listening");

    axum::serve(listener, router).await?;
    Ok(())
}

fn scopeball_home() -> PathBuf {
    if let Ok(p) = std::env::var("SCOPEBALL_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".scopeball")
}
