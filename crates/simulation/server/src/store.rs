//! In-memory stores — the DEV/TEST persistence backend.
//!
//! This is **not** the production store. The DB owner provides the SQLite-backed
//! [`WalletStore`] impl in the `simulation-db` crate; this crate wires that in
//! later. Until then, [`InMemoryWalletStore`] lets the server run end-to-end
//! against in-process maps/vectors.
//!
//! The wallet-state map and execution-report log are intentionally separate.
//! Simulation predictions are not authoritative wallet state; execution reports
//! record what happened after policy approval so a DB-backed reconciler can
//! later confirm them against chain receipts or venue snapshots.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use simulation_state::store::StoreError;
use simulation_state::{WalletId, WalletState, WalletStore};

use crate::dto::ExecutionReportRequest;

/// Records post-policy execution lifecycle events.
///
/// This boundary is separate from [`WalletStore`] because an execution report is
/// not, by itself, authoritative state. For example, a Hyperliquid venue
/// acceptance proves the venue saw an order request, but canonical open orders,
/// fills, and balances still come from a later venue sync snapshot.
#[async_trait]
#[allow(clippy::module_name_repetitions)]
pub trait ExecutionReportStore: Send + Sync {
    /// Persist one execution report for audit/reconciliation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing store cannot record the report.
    async fn record_execution_report(
        &self,
        report: ExecutionReportRequest,
    ) -> Result<(), StoreError>;
}

/// A process-local [`WalletStore`] backed by `Mutex`-protected collections.
///
/// Intended for development and tests. State lives only for the lifetime of the
/// process; restart loses everything. Swap for the DB owner's `SQLite` store in
/// production.
#[derive(Debug, Default)]
pub struct InMemoryWalletStore {
    wallets: Mutex<HashMap<WalletId, WalletState>>,
    execution_reports: Mutex<Vec<ExecutionReportRequest>>,
}

impl InMemoryWalletStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds (inserts or overwrites) a wallet's state.
    ///
    /// Test/dev helper — primes the store so a simulation runs against a known
    /// starting state instead of the empty default.
    ///
    /// # Panics
    ///
    /// Panics only if the internal mutex was poisoned by a prior panic while a
    /// lock was held — which cannot happen in normal operation.
    pub fn seed(&self, state: WalletState) {
        self.wallets
            .lock()
            .expect("InMemoryWalletStore mutex poisoned")
            .insert(state.wallet_id.clone(), state);
    }

    /// Returns a snapshot of recorded execution reports.
    ///
    /// Test/dev helper only. Production code should read reports from its DB
    /// implementation of [`ExecutionReportStore`].
    ///
    /// # Panics
    ///
    /// Panics only if the internal mutex was poisoned by a prior panic while a
    /// lock was held — which cannot happen in normal operation.
    #[must_use]
    pub fn execution_reports(&self) -> Vec<ExecutionReportRequest> {
        self.execution_reports
            .lock()
            .expect("InMemoryWalletStore mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl WalletStore for InMemoryWalletStore {
    async fn list_wallets(&self) -> Result<Vec<WalletId>, StoreError> {
        Ok(self
            .wallets
            .lock()
            .expect("InMemoryWalletStore mutex poisoned")
            .keys()
            .cloned()
            .collect())
    }

    /// Returns the stored state, or — for a wallet never seen before — a fresh
    /// empty [`WalletState::new`] for that id. This "first-seen" behavior lets a
    /// brand-new wallet simulate against empty state rather than erroring.
    /// Evaluation callers must not persist their predictions; authoritative
    /// sync/reconciliation callers use [`WalletStore::save`] when they have real
    /// ledger or venue state.
    async fn load(&self, id: &WalletId) -> Result<WalletState, StoreError> {
        Ok(self
            .wallets
            .lock()
            .expect("InMemoryWalletStore mutex poisoned")
            .get(id)
            .cloned()
            .unwrap_or_else(|| WalletState::new(id.clone())))
    }

    async fn save(&self, state: &WalletState) -> Result<(), StoreError> {
        self.wallets
            .lock()
            .expect("InMemoryWalletStore mutex poisoned")
            .insert(state.wallet_id.clone(), state.clone());
        Ok(())
    }
}

#[async_trait]
impl ExecutionReportStore for InMemoryWalletStore {
    async fn record_execution_report(
        &self,
        report: ExecutionReportRequest,
    ) -> Result<(), StoreError> {
        self.execution_reports
            .lock()
            .expect("InMemoryWalletStore mutex poisoned")
            .push(report);
        Ok(())
    }
}
