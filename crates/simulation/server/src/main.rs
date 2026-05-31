//! `simulation-server` binary entry point.
//!
//! Starts the axum HTTP service: initializes tracing, wires an
//! [`InMemoryWalletStore`] or SQLite-backed stores into [`AppState`], builds
//! the router, and serves on `SIMULATION_SERVER_ADDR` (default
//! `127.0.0.1:8788`).

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use simulation_server::app::{build_router, AppState};
use simulation_server::db_store::SqliteExecutionReportStore;
use simulation_server::store::{ExecutionReportStore, InMemoryWalletStore};
use simulation_sync::{
    Orchestrator, Scheduler, SchedulerConfig, SqliteWalletStore, SyncConfig, WalletStore,
};

/// Default bind address. Port `8788` deliberately differs from the legacy
/// Node.js policy-rpc host (`8787`) so the two can run side-by-side during the
/// migration.
const DEFAULT_ADDR: &str = "127.0.0.1:8788";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,simulation_server=debug")),
        )
        .init();

    let (wallet_store, execution_reports): (Arc<dyn WalletStore>, Arc<dyn ExecutionReportStore>) =
        if let Ok(db_path) = std::env::var("SIMULATION_DB_PATH") {
            let pool = simulation_db::Pool::open(db_path)?;
            simulation_db::run_migrations(&pool)?;
            (
                Arc::new(SqliteWalletStore::new(pool.clone())),
                Arc::new(SqliteExecutionReportStore::new(pool)),
            )
        } else {
            let store = Arc::new(InMemoryWalletStore::new());
            (store.clone(), store)
        };

    maybe_start_scheduler(wallet_store.clone())?;

    let state = AppState {
        store: wallet_store,
        execution_reports,
    };
    let router = build_router(state);

    let addr = std::env::var("SIMULATION_SERVER_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "simulation-server listening");

    axum::serve(listener, router).await?;
    Ok(())
}

fn maybe_start_scheduler(
    wallet_store: Arc<dyn WalletStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Ok(sync_config_path) = std::env::var("SIMULATION_SYNC_CONFIG") else {
        return Ok(());
    };
    let cfg = SyncConfig::load_file(sync_config_path)?;
    let orchestrator = Arc::new(Orchestrator::from_sync_config(&cfg)?);
    let tick_interval = std::env::var("SIMULATION_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or_else(|| SchedulerConfig::default().tick_interval);
    let scheduler = Scheduler::new(
        orchestrator,
        wallet_store,
        SchedulerConfig {
            tick_interval,
            ..SchedulerConfig::default()
        },
    );

    tokio::spawn(async move {
        if let Err(err) = scheduler.run_forever().await {
            tracing::warn!(%err, "simulation sync scheduler stopped");
        }
    });
    tracing::info!(?tick_interval, "simulation sync scheduler started");
    Ok(())
}
