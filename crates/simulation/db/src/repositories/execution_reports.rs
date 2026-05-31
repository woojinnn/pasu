//! `execution_reports` table CRUD.
//!
//! Reports are lifecycle facts observed after a policy decision. They are not
//! canonical state deltas. A later chain/venue sync reconciles attributed
//! reports against authoritative state and marks those rows reconciled.

use rusqlite::{params, Transaction};
use serde_json::Value as JsonValue;

use crate::error::DbResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportStage {
    Wallet,
    Onchain,
    Venue,
    Failure,
}

impl ReportStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wallet => "wallet",
            Self::Onchain => "onchain",
            Self::Venue => "venue",
            Self::Failure => "failure",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    WalletRejected,
    WalletSigned,
    OnchainSubmitted,
    OnchainConfirmed,
    VenueSubmitted,
    VenueAccepted,
    VenueRejected,
    Failed,
}

impl OutcomeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WalletRejected => "wallet_rejected",
            Self::WalletSigned => "wallet_signed",
            Self::OnchainSubmitted => "onchain_submitted",
            Self::OnchainConfirmed => "onchain_confirmed",
            Self::VenueSubmitted => "venue_submitted",
            Self::VenueAccepted => "venue_accepted",
            Self::VenueRejected => "venue_rejected",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExecutionReportInsert {
    pub wallet_id: Option<i64>,
    pub evaluation_id: Option<String>,
    pub action_index: Option<usize>,
    pub stage: ReportStage,
    pub outcome_kind: OutcomeKind,
    pub chain: Option<String>,
    pub tx_hash: Option<String>,
    pub signature: Option<String>,
    pub venue: Option<String>,
    pub venue_order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub reason: Option<String>,
    pub raw_json: JsonValue,
    pub metadata_json: JsonValue,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReportRow {
    pub id: i64,
    pub wallet_id: Option<i64>,
    pub evaluation_id: Option<String>,
    pub action_index: Option<i64>,
    pub stage: String,
    pub outcome_kind: String,
    pub chain: Option<String>,
    pub tx_hash: Option<String>,
    pub signature: Option<String>,
    pub venue: Option<String>,
    pub venue_order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub reason: Option<String>,
    pub raw_json: String,
    pub metadata_json: String,
    pub created_at: i64,
    pub reconciled_at: Option<i64>,
}

pub fn insert(tx: &Transaction<'_>, r: &ExecutionReportInsert) -> DbResult<i64> {
    let raw_json = serde_json::to_string(&r.raw_json)?;
    let metadata_json = serde_json::to_string(&r.metadata_json)?;
    let action_index = r
        .action_index
        .map(i64::try_from)
        .transpose()
        .map_err(|_| crate::error::DbError::Invariant("action_index overflow".into()))?;
    tx.execute(
        "INSERT INTO execution_reports \
           (wallet_id, evaluation_id, action_index, stage, outcome_kind, chain, tx_hash, \
            signature, venue, venue_order_id, client_order_id, reason, raw_json, metadata_json, \
            created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            r.wallet_id,
            r.evaluation_id,
            action_index,
            r.stage.as_str(),
            r.outcome_kind.as_str(),
            r.chain,
            r.tx_hash,
            r.signature,
            r.venue,
            r.venue_order_id,
            r.client_order_id,
            r.reason,
            raw_json,
            metadata_json,
            r.created_at,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

pub fn list_unreconciled_for_wallet(
    tx: &Transaction<'_>,
    wallet_id: i64,
) -> DbResult<Vec<ExecutionReportRow>> {
    let mut stmt = tx.prepare(
        "SELECT id, wallet_id, evaluation_id, action_index, stage, outcome_kind, chain, \
                tx_hash, signature, venue, venue_order_id, client_order_id, reason, raw_json, \
                metadata_json, created_at, reconciled_at \
         FROM execution_reports \
         WHERE wallet_id = ?1 AND reconciled_at IS NULL \
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], report_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn mark_reconciled_for_wallet(
    tx: &Transaction<'_>,
    wallet_id: i64,
    reconciled_at: i64,
) -> DbResult<usize> {
    // Only reconcile reports that existed before the sync snapshot started.
    // Reports arriving during the tick must wait for a later authoritative
    // chain/venue snapshot, otherwise we can mark a venue result reconciled by
    // state that could not yet include it.
    let n = tx.execute(
        "UPDATE execution_reports \
         SET reconciled_at = ?2 \
         WHERE wallet_id = ?1 AND reconciled_at IS NULL AND created_at < ?2",
        params![wallet_id, reconciled_at],
    )?;
    Ok(n)
}

pub fn count_unreconciled(tx: &Transaction<'_>) -> DbResult<i64> {
    let n = tx.query_row(
        "SELECT COUNT(*) FROM execution_reports WHERE reconciled_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(n)
}

fn report_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionReportRow> {
    Ok(ExecutionReportRow {
        id: r.get(0)?,
        wallet_id: r.get(1)?,
        evaluation_id: r.get(2)?,
        action_index: r.get(3)?,
        stage: r.get(4)?,
        outcome_kind: r.get(5)?,
        chain: r.get(6)?,
        tx_hash: r.get(7)?,
        signature: r.get(8)?,
        venue: r.get(9)?,
        venue_order_id: r.get(10)?,
        client_order_id: r.get(11)?,
        reason: r.get(12)?,
        raw_json: r.get(13)?,
        metadata_json: r.get(14)?,
        created_at: r.get(15)?,
        reconciled_at: r.get(16)?,
    })
}
