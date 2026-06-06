use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

use policy_state::{Time, WalletId, WalletStore};

use crate::error::SyncError;
use crate::orchestrator::{Orchestrator, RefreshReport};

#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub tick_interval: Duration,
    pub max_wallets_per_tick: usize,
    /// Refresh plain facts such as block heights, balances, and allowances.
    pub sync_primitives: bool,
    /// Refresh Hyperliquid account snapshots from the venue API.
    pub sync_hyperliquid_accounts: bool,
    /// Refresh `UniswapX` (and other intent-venue) order status.
    pub sync_intent_orders: bool,
    /// Refresh stale `LiveField` values.
    pub refresh_live_fields: bool,
    /// Run the HL long-tail sync (staking/vaults/borrow-lend/agents) once every
    /// N ticks; the fast core sync runs every tick. Values below 1 act as 1.
    pub hl_longtail_every: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(15),
            max_wallets_per_tick: 100,
            sync_primitives: true,
            sync_hyperliquid_accounts: true,
            sync_intent_orders: true,
            refresh_live_fields: true,
            hl_longtail_every: 10,
        }
    }
}

/// Per-wallet refresh summary surfaced from a tick so callers can emit a real
/// `wallet_synced` payload instead of zeros.
#[derive(Debug, Clone)]
pub struct WalletSyncCounts {
    pub wallet: WalletId,
    pub fields_updated: usize,
    pub fields_failed: usize,
}

#[derive(Debug, Default, Clone)]
pub struct TickReport {
    pub wallets_processed: usize,
    pub total_primitives_updated: usize,
    pub total_primitive_errors: usize,
    pub total_hyperliquid_accounts_updated: usize,
    pub total_hyperliquid_errors: usize,
    pub total_fields_updated: usize,
    pub total_fields_failed: usize,
    pub errors: Vec<String>,
    /// Per-wallet refresh summaries for wallets persisted this tick. Surfaced
    /// (beyond the aggregate counters) so the caller can emit one real-time
    /// `wallet_synced` event per wallet with real counts.
    pub synced_wallets: Vec<WalletSyncCounts>,
}

pub struct Scheduler {
    orchestrator: Arc<Orchestrator>,
    store: Arc<dyn WalletStore>,
    config: SchedulerConfig,
    stop: watch::Sender<bool>,
    /// Monotonic tick counter for sub-cadence scheduling (e.g. HL long-tail).
    tick_index: AtomicU64,
}

impl Scheduler {
    pub fn new(
        orchestrator: Arc<Orchestrator>,
        store: Arc<dyn WalletStore>,
        config: SchedulerConfig,
    ) -> Self {
        let (stop, _) = watch::channel(false);
        Self {
            orchestrator,
            store,
            config,
            stop,
            tick_index: AtomicU64::new(0),
        }
    }

    pub async fn tick_once(&self) -> Result<TickReport, SyncError> {
        let wallets = self.store.list_wallets().await?;
        let mut report = TickReport::default();
        let now = Time::from_unix(unix_now());
        let limit = self.config.max_wallets_per_tick;
        let tick = self.tick_index.fetch_add(1, Ordering::Relaxed);

        for wid in wallets.into_iter().take(limit) {
            let mut state = match self.store.load(&wid).await {
                Ok(state) => state,
                Err(e) => {
                    report.errors.push(format!("load {}: {}", wid.address, e));
                    continue;
                }
            };

            let mut w_updated: usize = 0;
            let mut w_failed: usize = 0;

            if self.config.sync_primitives {
                match self.orchestrator.sync_primitives(&mut state, now).await {
                    Ok(pr) => {
                        let updated = pr.block_heights_updated
                            + pr.native_balances_updated
                            + pr.erc20_balances_updated
                            + pr.approvals_updated;
                        report.total_primitives_updated += updated;
                        w_updated += updated;
                        report.total_primitive_errors += pr.errors.len();
                        report.errors.extend(
                            pr.errors
                                .into_iter()
                                .map(|e| format!("primitives {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => {
                        report.total_primitive_errors += 1;
                        w_failed += 1;
                        report
                            .errors
                            .push(format!("primitives {}: {}", wid.address, e));
                    }
                }
            }

            if self.config.sync_hyperliquid_accounts {
                // Core (fast): every tick.
                match self
                    .orchestrator
                    .sync_hyperliquid_core(&mut state, now)
                    .await
                {
                    Ok(hr) => {
                        if hr.account_updated {
                            report.total_hyperliquid_accounts_updated += 1;
                            w_updated += 1;
                        }
                        w_failed += hr.errors.len();
                        report.total_hyperliquid_errors += hr.errors.len();
                        report.errors.extend(
                            hr.errors
                                .into_iter()
                                .map(|e| format!("hyperliquid core {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => {
                        report.total_hyperliquid_errors += 1;
                        w_failed += 1;
                        report
                            .errors
                            .push(format!("hyperliquid core {}: {}", wid.address, e));
                    }
                }

                // Long-tail (slow): every Nth tick (initial tick included).
                if tick.is_multiple_of(self.config.hl_longtail_every.max(1)) {
                    match self
                        .orchestrator
                        .sync_hyperliquid_longtail(&mut state, now)
                        .await
                    {
                        Ok(hr) => {
                            report.total_hyperliquid_errors += hr.errors.len();
                            report.errors.extend(
                                hr.errors
                                    .into_iter()
                                    .map(|e| format!("hyperliquid longtail {}: {e}", wid.address)),
                            );
                        }
                        Err(e) => {
                            report.total_hyperliquid_errors += 1;
                            report
                                .errors
                                .push(format!("hyperliquid longtail {}: {}", wid.address, e));
                        }
                    }
                }
            }

            if self.config.sync_intent_orders {
                match self.orchestrator.sync_intent_orders(&mut state, now).await {
                    Ok(ir) => {
                        w_updated += ir.orders_updated;
                        w_failed += ir.errors.len();
                        report.errors.extend(
                            ir.errors
                                .into_iter()
                                .map(|e| format!("intent {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => {
                        w_failed += 1;
                        report.errors.push(format!("intent {}: {}", wid.address, e));
                    }
                }
            }

            // Permit/permit2 lifecycle reconciliation runs alongside intent
            // sync (a separate step, gated on the same flag) so signed permits
            // are retired on the same cadence as venue orders.
            if self.config.sync_intent_orders {
                match self.orchestrator.reconcile_permits(&mut state, now).await {
                    Ok(pr) => {
                        w_updated += pr.permits_retired;
                        w_failed += pr.errors.len();
                        report.errors.extend(
                            pr.errors
                                .into_iter()
                                .map(|e| format!("permit {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => {
                        w_failed += 1;
                        report.errors.push(format!("permit {}: {}", wid.address, e));
                    }
                }
            }

            if self.config.refresh_live_fields {
                match self.orchestrator.refresh(&mut state, now).await {
                    Ok(rr) => {
                        report.total_fields_updated += rr.fields_updated;
                        report.total_fields_failed += rr.fields_failed;
                        w_updated += rr.fields_updated;
                        w_failed += rr.fields_failed;
                        report.errors.extend(
                            rr.errors
                                .into_iter()
                                .map(|e| format!("refresh {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => report
                        .errors
                        .push(format!("refresh {}: {}", wid.address, e)),
                }
            }

            match self.store.save(&state).await {
                Ok(()) => {
                    report.wallets_processed += 1;
                    report.synced_wallets.push(WalletSyncCounts {
                        wallet: wid,
                        fields_updated: w_updated,
                        fields_failed: w_failed,
                    });
                }
                Err(e) => report.errors.push(format!("save {}: {}", wid.address, e)),
            }
        }
        Ok(report)
    }

    pub async fn run_forever(&self) -> Result<(), SyncError> {
        let mut stop_rx = self.stop.subscribe();
        loop {
            tokio::select! {
                () = tokio::time::sleep(self.config.tick_interval) => {
                    let _ = self.tick_once().await;
                }
                changed = stop_rx.changed() => {
                    if changed.is_ok() && *stop_rx.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn stop_handle(&self) -> watch::Sender<bool> {
        self.stop.clone()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[allow(dead_code)]
fn _refresh_report_keep() -> RefreshReport {
    RefreshReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use policy_state::store::StoreError;
    use policy_state::{Address, ChainId, WalletId, WalletState};

    struct MemStore {
        wallets: Mutex<HashMap<WalletId, WalletState>>,
    }

    #[async_trait]
    impl WalletStore for MemStore {
        async fn list_wallets(&self) -> Result<Vec<WalletId>, StoreError> {
            Ok(self.wallets.lock().unwrap().keys().cloned().collect())
        }
        async fn load(&self, id: &WalletId) -> Result<WalletState, StoreError> {
            self.wallets
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| StoreError::NotFound(id.clone()))
        }
        async fn save(&self, state: &WalletState) -> Result<(), StoreError> {
            self.wallets
                .lock()
                .unwrap()
                .insert(state.wallet_id.clone(), state.clone());
            Ok(())
        }
    }

    fn mk_scheduler() -> Scheduler {
        let toml = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let orch = Arc::new(Orchestrator::from_rpc_router(router));

        let wid = WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]);
        let state = WalletState::new(wid.clone());
        let mut map = HashMap::new();
        map.insert(wid, state);

        let store = Arc::new(MemStore {
            wallets: Mutex::new(map),
        });

        Scheduler::new(
            orch,
            store,
            SchedulerConfig {
                sync_primitives: false,
                sync_hyperliquid_accounts: false,
                ..SchedulerConfig::default()
            },
        )
    }

    #[tokio::test]
    async fn tick_processes_wallets() {
        let s = mk_scheduler();
        let report = s.tick_once().await.unwrap();
        assert_eq!(report.wallets_processed, 1);
        assert_eq!(report.total_fields_updated, 0); // empty state
    }

    #[tokio::test]
    async fn tick_records_synced_wallets() {
        // The background worker needs to know *which* wallets a tick refreshed
        // so it can emit one `wallet_synced` event per wallet. The aggregate
        // counters can't carry that, so the tick surfaces the wallet ids that
        // were successfully loaded, refreshed, and saved.
        let s = mk_scheduler();
        let report = s.tick_once().await.unwrap();
        assert_eq!(report.wallets_processed, 1);
        assert_eq!(report.synced_wallets.len(), 1);
        assert_eq!(report.synced_wallets[0].wallet.address, Address::ZERO);
    }

    #[tokio::test]
    async fn tick_runs_primitives_and_hyperliquid_sync_before_livefield_refresh() {
        let toml = r#"
[chains."eip155:1"]
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let orch = Arc::new(Orchestrator::new(crate::fetchers::OnchainViewFetcher::new(
            router,
        )));

        let wid = WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]);
        let state = WalletState::new(wid.clone());
        let mut map = HashMap::new();
        map.insert(wid, state);
        let store = Arc::new(MemStore {
            wallets: Mutex::new(map),
        });

        let s = Scheduler::new(orch, store, SchedulerConfig::default());
        let report = s.tick_once().await.unwrap();

        assert_eq!(report.wallets_processed, 1);
        assert_eq!(report.total_primitive_errors, 1);
        // Unconfigured HL on tick 0: core + long-tail both report "not configured".
        assert_eq!(report.total_hyperliquid_errors, 2);
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("hyperliquid fetcher is not configured")));
    }
}
