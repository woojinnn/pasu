use std::sync::Arc;
use std::time::Duration;

use policy_server::config::ServerConfig;
use policy_server::coordination::build_coordinator;
use policy_server::storage::StorageBackend;
use policy_sync::{Orchestrator, Scheduler, SchedulerConfig, SyncConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,policy_server=debug")),
        )
        .init();

    let config = ServerConfig::from_env();
    let storage = StorageBackend::open(&config).await?;

    let sync_config_path = std::env::var("SCOPEBALL_SYNC_CONFIG")
        .unwrap_or_else(|_| "./scopeball-sync.toml".to_owned());
    let sync_config = SyncConfig::load_file(sync_config_path)?;
    let orchestrator = Arc::new(Orchestrator::from_sync_config(&sync_config)?);
    let coordinator = build_coordinator(&config).await?;

    let tick_interval = Duration::from_secs(
        std::env::var("SYNC_WORKER_TICK_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
    );
    let sync_lock_ttl = Duration::from_secs(config.sync_lock_ttl_secs);

    loop {
        for user_id in storage.list_user_ids().await? {
            let lock_key = format!("sync:user:{user_id}");
            let Some(lock) = coordinator.try_lock(&lock_key, sync_lock_ttl).await? else {
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
            let report_result = scheduler.tick_once().await;
            coordinator.release_lock(lock).await?;
            let report = report_result?;
            tracing::info!(
                user_id,
                wallets = report.wallets_processed,
                errors = report.errors.len(),
                "sync worker tick complete"
            );
        }
        tokio::time::sleep(tick_interval).await;
    }
}
