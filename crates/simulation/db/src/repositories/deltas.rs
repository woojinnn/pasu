//! `state_deltas` 테이블 CRUD — live / backfill 통합 lifecycle 로그.
//!
//! INSERT 위주 (append-only); status 전이는 UPDATE 헬퍼로 명시.

use rusqlite::{Transaction, params};
use serde_json::Value as JsonValue;

use crate::error::DbResult;

/// 출처.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaSource {
    /// 익스텐션이 가로챈 tx — predicted → pending → confirmed lifecycle.
    Live,
    /// 과거 chain 에서 가져온 tx — historical 단일 status.
    Backfill,
}

impl DeltaSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Backfill => "backfill",
        }
    }
}

/// 라이프사이클 status. source 가 결정하는 valid set 이 다름.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaStatus {
    /// (live) reducer + Cedar 시뮬레이션 완료, 사용자 사인 전.
    Predicted,
    /// (live) 사인 후 멤풀 대기.
    Pending,
    /// (live) 블록 확정, state 테이블 반영 완료. (backfill) chain 에서 발견.
    Confirmed,
    /// (live) revert 등 실패.
    Failed,
    /// (backfill) 과거 chain 스캔으로 발견한 tx.
    Historical,
    /// 공통 — 리오그 등으로 무효화.
    RolledBack,
}

impl DeltaStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Predicted => "predicted",
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Historical => "historical",
            Self::RolledBack => "rolled_back",
        }
    }
}

/// INSERT 입력. JSON 필드는 호출자가 직렬화해서 넘김 (Action/StateDelta 타입은
/// reducer 의 것 — DB 는 opaque blob 으로 취급).
#[derive(Clone, Debug)]
pub struct DeltaInsert {
    pub wallet_id: i64,
    pub source: DeltaSource,
    pub status: DeltaStatus,
    pub created_at: i64,
    pub signed_at: Option<i64>,
    pub confirmed_at: Option<i64>,
    pub action_domain: String,
    pub action_kind: String,
    pub submitter: String,
    pub nature_kind: String,
    pub chain: Option<String>,
    pub nonce: Option<i64>,
    pub action_json: JsonValue,
    pub predicted_delta_json: Option<JsonValue>,
    pub predicted_verdict: Option<String>,
    pub predicted_verdict_reasons_json: Option<JsonValue>,
    pub tx_hash: Option<String>,
    pub sig_hash: Option<String>,
    pub realized_block_number: Option<i64>,
    pub realized_delta_json: Option<JsonValue>,
}

/// 새 delta 한 row INSERT. id 반환.
pub fn insert(tx: &Transaction<'_>, d: &DeltaInsert) -> DbResult<i64> {
    let action_str = serde_json::to_string(&d.action_json)?;
    let pred_delta = d
        .predicted_delta_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let pred_reasons = d
        .predicted_verdict_reasons_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let real_delta = d
        .realized_delta_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    tx.execute(
        "INSERT INTO state_deltas ( \
            wallet_id, source, status, \
            created_at, signed_at, confirmed_at, \
            action_domain, action_kind, submitter, nature_kind, chain, nonce, action_json, \
            predicted_delta_json, predicted_verdict, predicted_verdict_reasons_json, \
            tx_hash, sig_hash, realized_block_number, realized_delta_json \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, \
                   ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            d.wallet_id,
            d.source.as_str(),
            d.status.as_str(),
            d.created_at,
            d.signed_at,
            d.confirmed_at,
            d.action_domain,
            d.action_kind,
            d.submitter,
            d.nature_kind,
            d.chain,
            d.nonce,
            action_str,
            pred_delta,
            d.predicted_verdict,
            pred_reasons,
            d.tx_hash,
            d.sig_hash,
            d.realized_block_number,
            real_delta,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

/// status 전이 — `predicted` → `pending`.
pub fn mark_pending(
    tx: &Transaction<'_>,
    id: i64,
    signed_at: i64,
    tx_hash: &str,
) -> DbResult<()> {
    tx.execute(
        "UPDATE state_deltas SET status = 'pending', signed_at = ?2, tx_hash = ?3 \
         WHERE id = ?1 AND status = 'predicted'",
        params![id, signed_at, tx_hash],
    )?;
    Ok(())
}

/// status 전이 — `pending` → `confirmed`.
pub fn mark_confirmed(
    tx: &Transaction<'_>,
    id: i64,
    confirmed_at: i64,
    realized_block: i64,
    realized_delta_json: &JsonValue,
) -> DbResult<()> {
    let real = serde_json::to_string(realized_delta_json)?;
    tx.execute(
        "UPDATE state_deltas SET status = 'confirmed', confirmed_at = ?2, \
            realized_block_number = ?3, realized_delta_json = ?4 \
         WHERE id = ?1 AND status = 'pending'",
        params![id, confirmed_at, realized_block, real],
    )?;
    Ok(())
}

/// status 전이 — `pending` → `failed`.
pub fn mark_failed(tx: &Transaction<'_>, id: i64, reason: &str) -> DbResult<()> {
    tx.execute(
        "UPDATE state_deltas SET status = 'failed', failure_reason = ?2 \
         WHERE id = ?1 AND status = 'pending'",
        params![id, reason],
    )?;
    Ok(())
}

/// 한 row 의 모든 정보 SELECT (`live` / `backfill` 공통).
#[derive(Clone, Debug)]
pub struct DeltaRow {
    pub id: i64,
    pub wallet_id: i64,
    pub source: String,
    pub status: String,
    pub created_at: i64,
    pub signed_at: Option<i64>,
    pub confirmed_at: Option<i64>,
    pub action_domain: String,
    pub action_kind: String,
    pub submitter: String,
    pub tx_hash: Option<String>,
    pub predicted_verdict: Option<String>,
    pub action_json: String,
    pub predicted_delta_json: Option<String>,
    pub realized_delta_json: Option<String>,
}

/// 한 wallet 의 최신 N개.
pub fn list_recent(tx: &Transaction<'_>, wallet_id: i64, limit: i64) -> DbResult<Vec<DeltaRow>> {
    let mut stmt = tx.prepare(
        "SELECT id, wallet_id, source, status, created_at, signed_at, confirmed_at, \
                action_domain, action_kind, submitter, tx_hash, predicted_verdict, \
                action_json, predicted_delta_json, realized_delta_json \
         FROM state_deltas \
         WHERE wallet_id = ?1 \
         ORDER BY created_at DESC \
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![wallet_id, limit], |r| {
            Ok(DeltaRow {
                id: r.get(0)?,
                wallet_id: r.get(1)?,
                source: r.get(2)?,
                status: r.get(3)?,
                created_at: r.get(4)?,
                signed_at: r.get(5)?,
                confirmed_at: r.get(6)?,
                action_domain: r.get(7)?,
                action_kind: r.get(8)?,
                submitter: r.get(9)?,
                tx_hash: r.get(10)?,
                predicted_verdict: r.get(11)?,
                action_json: r.get(12)?,
                predicted_delta_json: r.get(13)?,
                realized_delta_json: r.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 한 wallet 의 status 별 카운트.
pub fn count_by_status(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<(String, i64)>> {
    let mut stmt = tx.prepare(
        "SELECT status, COUNT(*) FROM state_deltas WHERE wallet_id = ?1 GROUP BY status",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 한 wallet 의 총 delta 수.
pub fn count_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<i64> {
    let n: i64 = tx.query_row(
        "SELECT COUNT(*) FROM state_deltas WHERE wallet_id = ?1",
        params![wallet_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::Pool;
    use crate::repositories::wallets::{insert as insert_wallet, WalletInsert};
    use simulation_state::primitives::ChainId;

    fn fresh_pool() -> Pool {
        let pool = Pool::open_in_memory();
        crate::run_migrations(&pool).unwrap();
        pool
    }

    fn ins_wallet(tx: &Transaction<'_>, addr: &str) -> i64 {
        insert_wallet(
            tx,
            &WalletInsert {
                address: addr.into(),
                label: None,
                is_owned: true,
                created_at: 1_700_000_000,
                chains: vec![ChainId::ethereum_mainnet()],
            },
        )
        .unwrap()
    }

    fn sample_predicted(wallet_id: i64) -> DeltaInsert {
        DeltaInsert {
            wallet_id,
            source: DeltaSource::Live,
            status: DeltaStatus::Predicted,
            created_at: 1_738_000_000,
            signed_at: None,
            confirmed_at: None,
            action_domain: "lending".into(),
            action_kind: "borrow".into(),
            submitter: "0xowner".into(),
            nature_kind: "onchain_tx".into(),
            chain: Some("eip155:1".into()),
            nonce: Some(47),
            action_json: serde_json::json!({"body": "stub"}),
            predicted_delta_json: Some(serde_json::json!({"tokens": {}})),
            predicted_verdict: Some("allow".into()),
            predicted_verdict_reasons_json: None,
            tx_hash: None,
            sig_hash: None,
            realized_block_number: None,
            realized_delta_json: None,
        }
    }

    #[test]
    fn insert_and_list() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            let id = insert(tx, &sample_predicted(w))?;
            assert!(id >= 1);

            let rows = list_recent(tx, w, 10)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].status, "predicted");
            assert_eq!(rows[0].predicted_verdict.as_deref(), Some("allow"));
            assert_eq!(count_for_wallet(tx, w)?, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn lifecycle_transitions() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            let id = insert(tx, &sample_predicted(w))?;

            mark_pending(tx, id, 1_738_000_100, "0xabc...")?;
            mark_confirmed(
                tx,
                id,
                1_738_000_200,
                25_197_950,
                &serde_json::json!({"tokens": {"USDC": "+500"}}),
            )?;

            let rows = list_recent(tx, w, 10)?;
            assert_eq!(rows[0].status, "confirmed");
            assert_eq!(rows[0].tx_hash.as_deref(), Some("0xabc..."));
            assert!(rows[0].realized_delta_json.is_some());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn count_by_status_groups() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            for _ in 0..3 {
                insert(tx, &sample_predicted(w))?;
            }
            let mut d = sample_predicted(w);
            d.status = DeltaStatus::Confirmed;
            d.confirmed_at = Some(1_738_000_100);
            d.tx_hash = Some("0xdef...".into());
            d.realized_delta_json = Some(serde_json::json!({}));
            insert(tx, &d)?;

            let counts = count_by_status(tx, w)?;
            let p = counts.iter().find(|(s, _)| s == "predicted").unwrap().1;
            let c = counts.iter().find(|(s, _)| s == "confirmed").unwrap().1;
            assert_eq!(p, 3);
            assert_eq!(c, 1);
            Ok(())
        })
        .unwrap();
    }
}
