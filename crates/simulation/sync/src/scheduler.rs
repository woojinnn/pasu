//! Scheduler — 백그라운드에서 주기적으로 orchestrator.refresh 호출.
//!
//! Sync orchestrator 는 stateless 라 wallet 목록과 wallet load/save 는 호출자가
//! [`WalletStore`] trait 으로 제공. DB 와 직접 결합 회피 — `simulation-db` 가
//! 그 trait 을 impl 해서 주입.
//!
//! tick 마다 `list_wallets()` → 각각 load → refresh → save.
//! 실패한 wallet 은 errors 에 누적, 전체 루프는 멈추지 않음.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

use simulation_state::{Time, WalletStore};

use crate::error::SyncError;
use crate::orchestrator::{Orchestrator, RefreshReport};

#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub tick_interval: Duration,
    /// 한 tick 안에서 같은 wallet 을 다시 처리하지 않도록 batch size 제한.
    pub max_wallets_per_tick: usize,
    /// Refresh plain facts such as block heights, balances, and allowances.
    pub sync_primitives: bool,
    /// Refresh Hyperliquid account snapshots from the venue API.
    pub sync_hyperliquid_accounts: bool,
    /// Refresh stale `LiveField` values.
    pub refresh_live_fields: bool,
    /// Reconcile stored execution reports after an authoritative sync updates a
    /// wallet snapshot.
    pub reconcile_reports: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(15),
            max_wallets_per_tick: 100,
            sync_primitives: true,
            sync_hyperliquid_accounts: true,
            refresh_live_fields: true,
            reconcile_reports: true,
        }
    }
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
    pub total_reports_reconciled: usize,
    pub errors: Vec<String>,
}

pub struct Scheduler {
    orchestrator: Arc<Orchestrator>,
    store: Arc<dyn WalletStore>,
    config: SchedulerConfig,
    /// shutdown 신호.
    stop: watch::Sender<bool>,
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
        }
    }

    /// 한 tick 만 수동 실행 (테스트 / on-demand 용).
    pub async fn tick_once(&self) -> Result<TickReport, SyncError> {
        let wallets = self.store.list_wallets().await?;
        let mut report = TickReport::default();
        let now = Time::from_unix(unix_now());
        let limit = self.config.max_wallets_per_tick;

        for wid in wallets.into_iter().take(limit) {
            let mut state = match self.store.load(&wid).await {
                Ok(state) => state,
                Err(e) => {
                    report.errors.push(format!("load {}: {}", wid.address, e));
                    continue;
                }
            };

            let mut authoritative_updated = false;

            if self.config.sync_primitives {
                match self.orchestrator.sync_primitives(&mut state, now).await {
                    Ok(pr) => {
                        let updated = pr.block_heights_updated
                            + pr.native_balances_updated
                            + pr.erc20_balances_updated
                            + pr.approvals_updated;
                        report.total_primitives_updated += updated;
                        report.total_primitive_errors += pr.errors.len();
                        authoritative_updated |= updated > 0;
                        report.errors.extend(
                            pr.errors
                                .into_iter()
                                .map(|e| format!("primitives {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => {
                        report.total_primitive_errors += 1;
                        report
                            .errors
                            .push(format!("primitives {}: {}", wid.address, e));
                    }
                }
            }

            if self.config.sync_hyperliquid_accounts {
                match self
                    .orchestrator
                    .sync_hyperliquid_account(&mut state, now)
                    .await
                {
                    Ok(hr) => {
                        if hr.account_updated {
                            report.total_hyperliquid_accounts_updated += 1;
                            authoritative_updated = true;
                        }
                        report.total_hyperliquid_errors += hr.errors.len();
                        report.errors.extend(
                            hr.errors
                                .into_iter()
                                .map(|e| format!("hyperliquid {}: {e}", wid.address)),
                        );
                    }
                    Err(e) => {
                        report.total_hyperliquid_errors += 1;
                        report
                            .errors
                            .push(format!("hyperliquid {}: {}", wid.address, e));
                    }
                }
            }

            if self.config.refresh_live_fields {
                match self.orchestrator.refresh(&mut state, now).await {
                    Ok(rr) => {
                        report.total_fields_updated += rr.fields_updated;
                        report.total_fields_failed += rr.fields_failed;
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
                    if self.config.reconcile_reports && authoritative_updated {
                        match self.store.reconcile_reports(&wid, now).await {
                            Ok(n) => report.total_reports_reconciled += n,
                            Err(e) => report
                                .errors
                                .push(format!("reconcile {}: {}", wid.address, e)),
                        }
                    }
                }
                Err(e) => report.errors.push(format!("save {}: {}", wid.address, e)),
            }
        }
        Ok(report)
    }

    /// 무한 루프. `stop_handle()` 으로 종료 가능.
    pub async fn run_forever(&self) -> Result<(), SyncError> {
        let mut stop_rx = self.stop.subscribe();
        loop {
            tokio::select! {
                () = tokio::time::sleep(self.config.tick_interval) => {
                    let _ = self.tick_once().await; // 에러는 tick report 에 누적, 로그 X
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
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// `RefreshReport` 가 build 에 안 쓰이는 경고 회피 (export 보존).
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
    use simulation_state::store::StoreError;
    use simulation_state::{Address, ChainId, WalletId, WalletState};

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
        // dummy orchestrator (in-memory state 만 다루도록 router 는 publicnode)
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
        assert_eq!(report.total_hyperliquid_errors, 1);
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("hyperliquid fetcher is not configured")));
    }
}
