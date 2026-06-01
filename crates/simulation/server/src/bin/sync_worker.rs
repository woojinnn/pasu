use std::sync::Arc;
use std::time::Duration;

use simulation_server::config::ServerConfig;
use simulation_server::coordination::{Coordinator, NoopCoordinator};
use simulation_server::storage::StorageBackend;
use simulation_sync::{Orchestrator, Scheduler, SchedulerConfig, SyncConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,simulation_server=debug")),
        )
        .init();

    let config = ServerConfig::from_env();
    let home = scopeball_home();
    let storage = StorageBackend::open(&config, &home)?;

    let sync_config_path = std::env::var("SCOPEBALL_SYNC_CONFIG")
        .unwrap_or_else(|_| "./scopeball-sync.toml".to_owned());
    let sync_config = SyncConfig::load_file(sync_config_path)?;
    let orchestrator = Arc::new(Orchestrator::from_sync_config(&sync_config)?);
    let coordinator: Arc<dyn Coordinator> = Arc::new(NoopCoordinator);

    let tick_interval = Duration::from_secs(
        std::env::var("SYNC_WORKER_TICK_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
    );

    loop {
        for user_id in storage.list_user_ids()? {
            let lock_key = format!("sync:user:{user_id}");
            let Some(lock) = coordinator.try_lock(&lock_key, tick_interval * 2).await? else {
                continue;
            };

            let store = storage.wallet_store_for_user(&user_id)?;
            let scheduler = Scheduler::new(
                orchestrator.clone(),
                store,
                SchedulerConfig {
                    tick_interval,
                    ..SchedulerConfig::default()
                },
            );
            let report = scheduler.tick_once().await?;
            tracing::info!(
                user_id,
                wallets = report.wallets_processed,
                errors = report.errors.len(),
                "sync worker tick complete"
            );
            coordinator.release_lock(lock).await?;
        }
        tokio::time::sleep(tick_interval).await;
    }
}

fn scopeball_home() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("SCOPEBALL_HOME") {
        return std::path::PathBuf::from(p);
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".scopeball")
}
