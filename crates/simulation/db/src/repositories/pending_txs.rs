//! `pending_txs` CRUD — offchain signature ledger.
//!
//! `state_deltas.status`='pending' (mempool 의 onchain tx) 과 다름: 여기는 서명만
//! 했고 매처/리졸버/다른 서명자를 기다리는 의도.

use rusqlite::{params, Transaction};
use serde_json::Value as JsonValue;

use crate::error::DbResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTxRow {
    pub id: i64,
    pub wallet_id: i64,
    pub sig_hash: String,
    pub nature: String,
    pub chain: Option<String>,
    pub action_json: String,
    pub deadline: Option<i64>,
    pub nonce_key: Option<String>,
    pub signed_at: i64,
    pub matched_tx_hash: Option<String>,
    pub expired_at: Option<i64>,
    pub cancelled_at: Option<i64>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct PendingTxInsert {
    pub wallet_id: i64,
    pub sig_hash: String,
    pub nature: String,
    pub chain: Option<String>,
    pub action: JsonValue,
    pub deadline: Option<i64>,
    pub nonce_key: Option<String>,
    pub signed_at: i64,
}

pub fn insert(tx: &Transaction<'_>, p: &PendingTxInsert) -> DbResult<i64> {
    let action_s = serde_json::to_string(&p.action)?;
    tx.execute(
        "INSERT INTO pending_txs \
           (wallet_id, sig_hash, nature, chain, action_json, deadline, nonce_key, signed_at, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'awaiting')",
        params![
            p.wallet_id,
            p.sig_hash,
            p.nature,
            p.chain,
            action_s,
            p.deadline,
            p.nonce_key,
            p.signed_at,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

/// status 전이: awaiting → matched (onchain 으로 실현된 후).
pub fn mark_matched(tx: &Transaction<'_>, id: i64, matched_tx_hash: &str) -> DbResult<()> {
    tx.execute(
        "UPDATE pending_txs SET status='matched', matched_tx_hash=?2 \
         WHERE id=?1 AND status='awaiting'",
        params![id, matched_tx_hash],
    )?;
    Ok(())
}

pub fn mark_expired(tx: &Transaction<'_>, id: i64, at: i64) -> DbResult<()> {
    tx.execute(
        "UPDATE pending_txs SET status='expired', expired_at=?2 \
         WHERE id=?1 AND status='awaiting'",
        params![id, at],
    )?;
    Ok(())
}

pub fn mark_cancelled(tx: &Transaction<'_>, id: i64, at: i64) -> DbResult<()> {
    tx.execute(
        "UPDATE pending_txs SET status='cancelled', cancelled_at=?2 \
         WHERE id=?1 AND status='awaiting'",
        params![id, at],
    )?;
    Ok(())
}

pub fn list_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<PendingTxRow>> {
    let mut stmt = tx.prepare(
        "SELECT id, wallet_id, sig_hash, nature, chain, action_json, deadline, nonce_key, \
                signed_at, matched_tx_hash, expired_at, cancelled_at, status \
         FROM pending_txs WHERE wallet_id = ?1 \
         ORDER BY signed_at DESC",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| {
            Ok(PendingTxRow {
                id: r.get(0)?,
                wallet_id: r.get(1)?,
                sig_hash: r.get(2)?,
                nature: r.get(3)?,
                chain: r.get(4)?,
                action_json: r.get(5)?,
                deadline: r.get(6)?,
                nonce_key: r.get(7)?,
                signed_at: r.get(8)?,
                matched_tx_hash: r.get(9)?,
                expired_at: r.get(10)?,
                cancelled_at: r.get(11)?,
                status: r.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn count_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<i64> {
    let n: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pending_txs WHERE wallet_id = ?1",
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
    use serde_json::json;
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

    #[test]
    fn insert_and_lifecycle() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            let id = insert(
                tx,
                &PendingTxInsert {
                    wallet_id: w,
                    sig_hash: "0xdead".into(),
                    nature: "uniswapx_intent".into(),
                    chain: Some("eip155:1".into()),
                    action: json!({"swap": "1 ETH → USDC"}),
                    deadline: Some(1_900_000_000),
                    nonce_key: None,
                    signed_at: 1_738_000_000,
                },
            )?;
            let rows = list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].status, "awaiting");

            mark_matched(tx, id, "0xrealtx")?;
            let rows = list_for_wallet(tx, w)?;
            assert_eq!(rows[0].status, "matched");
            assert_eq!(rows[0].matched_tx_hash.as_deref(), Some("0xrealtx"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn expired_only_from_awaiting() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            let id = insert(
                tx,
                &PendingTxInsert {
                    wallet_id: w,
                    sig_hash: "0xabc".into(),
                    nature: "permit2".into(),
                    chain: None,
                    action: json!({}),
                    deadline: Some(1_700_000_000),
                    nonce_key: None,
                    signed_at: 1_699_000_000,
                },
            )?;
            mark_matched(tx, id, "0xtx")?;
            // 이제 matched. mark_expired 호출은 status='awaiting' AND 조건이라 no-op.
            mark_expired(tx, id, 1_800_000_000)?;
            let rows = list_for_wallet(tx, w)?;
            assert_eq!(rows[0].status, "matched");
            Ok(())
        })
        .unwrap();
    }
}
