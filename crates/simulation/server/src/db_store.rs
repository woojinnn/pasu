//! `SQLite` execution-report store.
//!
//! This adapter is intentionally narrow: wallet state persistence still lives
//! behind `simulation_sync::WalletStore`, while report persistence writes the
//! post-policy lifecycle facts into `simulation-db`.

use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use simulation_db::repositories::execution_reports::{
    self, ExecutionReportInsert, OutcomeKind, ReportStage,
};
use simulation_db::repositories::wallets::{self, WalletInsert};
use simulation_db::{DbError, Pool};
use simulation_sync::SyncError;

use crate::dto::{ExecutionReportOutcome, ExecutionReportRequest};
use crate::store::ExecutionReportStore;

/// SQLite-backed implementation of [`ExecutionReportStore`].
#[derive(Clone, Debug)]
pub struct SqliteExecutionReportStore {
    pool: Pool,
}

impl SqliteExecutionReportStore {
    /// Creates a report store over an already-migrated pool.
    #[must_use]
    pub const fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionReportStore for SqliteExecutionReportStore {
    async fn record_execution_report(
        &self,
        report: ExecutionReportRequest,
    ) -> Result<(), SyncError> {
        let created_at = unix_now_i64();
        self.pool.with_tx(|tx| {
            let wallet_row_id = match &report.wallet_id {
                Some(wallet_id) => {
                    let address = wallet_id.address.to_string().to_lowercase();
                    let id = if let Some(w) = wallets::get_by_address(tx, &address)? {
                        w.id
                    } else {
                        wallets::insert(
                            tx,
                            &WalletInsert {
                                address,
                                label: None,
                                is_owned: true,
                                created_at,
                                chains: wallet_id.chains.iter().cloned().collect(),
                            },
                        )?
                    };
                    Some(id)
                }
                None => None,
            };
            let insert = to_insert(report, wallet_row_id, created_at)?;
            execution_reports::insert(tx, &insert)?;
            Ok(())
        })?;
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn to_insert(
    report: ExecutionReportRequest,
    wallet_row_id: Option<i64>,
    created_at: i64,
) -> Result<ExecutionReportInsert, DbError> {
    let raw_json = serde_json::to_value(&report)?;
    let metadata_json = serde_json::to_value(&report.metadata)?;
    let (
        stage,
        outcome_kind,
        chain,
        tx_hash,
        signature,
        venue,
        venue_order_id,
        client_order_id,
        reason,
    ) = match &report.outcome {
        ExecutionReportOutcome::WalletRejected { reason } => (
            ReportStage::Wallet,
            OutcomeKind::WalletRejected,
            None,
            None,
            None,
            None,
            None,
            None,
            reason.clone(),
        ),
        ExecutionReportOutcome::WalletSigned { signature } => (
            ReportStage::Wallet,
            OutcomeKind::WalletSigned,
            None,
            None,
            Some(signature.clone()),
            None,
            None,
            None,
            None,
        ),
        ExecutionReportOutcome::OnchainSubmitted { chain, tx_hash } => (
            ReportStage::Onchain,
            OutcomeKind::OnchainSubmitted,
            Some(chain.to_string()),
            Some(tx_hash.clone()),
            None,
            None,
            None,
            None,
            None,
        ),
        ExecutionReportOutcome::OnchainConfirmed { chain, tx_hash, .. } => (
            ReportStage::Onchain,
            OutcomeKind::OnchainConfirmed,
            Some(chain.to_string()),
            Some(tx_hash.clone()),
            None,
            None,
            None,
            None,
            None,
        ),
        ExecutionReportOutcome::VenueSubmitted {
            venue,
            client_order_id,
        } => (
            ReportStage::Venue,
            OutcomeKind::VenueSubmitted,
            None,
            None,
            None,
            Some(venue.clone()),
            None,
            client_order_id.clone(),
            None,
        ),
        ExecutionReportOutcome::VenueAccepted {
            venue,
            venue_order_id,
            client_order_id,
        } => (
            ReportStage::Venue,
            OutcomeKind::VenueAccepted,
            None,
            None,
            None,
            Some(venue.clone()),
            venue_order_id.clone(),
            client_order_id.clone(),
            None,
        ),
        ExecutionReportOutcome::VenueRejected { venue, reason } => (
            ReportStage::Venue,
            OutcomeKind::VenueRejected,
            None,
            None,
            None,
            Some(venue.clone()),
            None,
            None,
            Some(reason.clone()),
        ),
        ExecutionReportOutcome::Failed { reason } => (
            ReportStage::Failure,
            OutcomeKind::Failed,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(reason.clone()),
        ),
    };

    Ok(ExecutionReportInsert {
        wallet_id: wallet_row_id,
        evaluation_id: report.evaluation_id,
        action_index: report.action_index,
        stage,
        outcome_kind,
        chain,
        tx_hash,
        signature,
        venue,
        venue_order_id,
        client_order_id,
        reason,
        raw_json,
        metadata_json,
        created_at,
    })
}

fn unix_now_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::str::FromStr;

    use simulation_db::run_migrations;
    use simulation_state::{Address, ChainId, WalletId};

    #[tokio::test]
    async fn sqlite_report_store_persists_unattributed_venue_reports() {
        let pool = Pool::open_in_memory();
        run_migrations(&pool).unwrap();
        let store = SqliteExecutionReportStore::new(pool.clone());

        store
            .record_execution_report(ExecutionReportRequest {
                wallet_id: None,
                evaluation_id: Some("eval-unattributed".into()),
                action_index: None,
                outcome: ExecutionReportOutcome::VenueAccepted {
                    venue: "hyperliquid".into(),
                    venue_order_id: Some("42".into()),
                    client_order_id: None,
                },
                metadata: BTreeMap::new(),
            })
            .await
            .unwrap();

        pool.with_tx(|tx| {
            assert_eq!(execution_reports::count_unreconciled(tx)?, 1);
            Ok(())
        })
        .unwrap();
    }

    #[tokio::test]
    async fn sqlite_report_store_persists_attributed_reports_for_reconciliation() {
        let pool = Pool::open_in_memory();
        run_migrations(&pool).unwrap();
        let store = SqliteExecutionReportStore::new(pool.clone());
        let wallet_id = WalletId::new(
            Address::from_str("0x362E7e9e630481631D7C804dfe50e24b53250925").unwrap(),
            [ChainId::ethereum_mainnet()],
        );

        store
            .record_execution_report(ExecutionReportRequest {
                wallet_id: Some(wallet_id),
                evaluation_id: Some("eval-attributed".into()),
                action_index: Some(0),
                outcome: ExecutionReportOutcome::VenueRejected {
                    venue: "hyperliquid".into(),
                    reason: "bad signature".into(),
                },
                metadata: serde_json::json!({"source":"test"})
                    .as_object()
                    .unwrap()
                    .clone()
                    .into_iter()
                    .collect(),
            })
            .await
            .unwrap();

        pool.with_tx(|tx| {
            let wallet =
                wallets::get_by_address(tx, "0x362e7e9e630481631d7c804dfe50e24b53250925")?.unwrap();
            let rows = execution_reports::list_unreconciled_for_wallet(tx, wallet.id)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].outcome_kind, "venue_rejected");
            Ok(())
        })
        .unwrap();
    }
}
