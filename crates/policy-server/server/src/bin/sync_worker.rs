use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use policy_server::config::ServerConfig;
use policy_server::coordination::build_coordinator;
use policy_server::events::{publish_tick_events, RedisEventPublisher};
use policy_server::storage::StorageBackend;
use policy_sync::{Orchestrator, Scheduler, SchedulerConfig, SyncConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let config = ServerConfig::from_env();
    policy_server::logging::init_tracing(config.log_format);
    let storage = StorageBackend::open(&config).await?;

    let sync_config_path = std::env::var("PASU_SYNC_CONFIG")
        .unwrap_or_else(|_| "./pasu-sync.toml".to_owned());
    let sync_config = SyncConfig::load_file(sync_config_path)?;
    let orchestrator = Arc::new(Orchestrator::from_sync_config(&sync_config)?);
    let coordinator = build_coordinator(&config).await?;

    // The worker is a separate process from the API, so a local in-process bus
    // would have no SSE subscribers. Live `wallet_synced` events therefore only
    // flow when Redis is configured — the API replicas forward them to their SSE
    // clients. Without Redis the worker still refreshes and persists state; it
    // simply can't push live notifications.
    let publisher: Option<RedisEventPublisher> = match config.redis_url.as_deref() {
        Some(url) if !url.trim().is_empty() => {
            match RedisEventPublisher::connect(url, config.redis_events_channel.clone()).await {
                Ok(p) => {
                    tracing::info!(
                        channel = %config.redis_events_channel,
                        "sync worker: Redis event fanout enabled"
                    );
                    Some(p)
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "sync worker: Redis connect failed — live wallet_synced events disabled"
                    );
                    None
                }
            }
        }
        _ => {
            tracing::info!(
                "sync worker: REDIS_URL not set — live wallet_synced events disabled (state still persisted)"
            );
            None
        }
    };

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

            // Push one live `wallet_synced` per refreshed wallet so dashboards
            // update without waiting for the next manual sync or page reload.
            if let Some(publisher) = &publisher {
                publish_tick_events(publisher, &user_id, &report.synced_wallets, unix_now()).await;
            }
        }
        tokio::time::sleep(tick_interval).await;
    }
}

/// Current Unix time in whole seconds, clamped to a non-negative `i64` for the
/// event payload. Returns 0 if the system clock is before the epoch.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}
