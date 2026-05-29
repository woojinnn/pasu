//! `positions` 테이블 CRUD — protocol-tracked 권리/상태.
//!
//! Phase 2.4 는 모든 variant 를 generic JSON 으로 저장. (per-kind 평탄화는
//! Phase 후속 — 그 시점에 정책 쿼리 가 어떤 컬럼을 자주 보는지 가늠 가능.)

use rusqlite::{Transaction, params};
use serde_json::Value as JsonValue;

use crate::error::DbResult;

/// 한 position row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionRow {
    pub id: i64,
    pub wallet_id: i64,
    pub position_id: String,
    pub protocol: String,
    pub chain: Option<String>,
    pub kind: String,
    pub market: Option<String>,
    pub summary: Option<String>,
    pub data_json: String,
    pub primitives_synced_at: i64,
    pub primitives_source_json: String,
}

/// INSERT 입력 (id 는 자동).
#[derive(Clone, Debug)]
pub struct PositionInsert {
    pub wallet_id: i64,
    pub position_id: String,
    pub protocol: String,
    pub chain: Option<String>,
    pub kind: String,
    pub market: Option<String>,
    pub summary: Option<String>,
    pub data: JsonValue,
    pub primitives_synced_at: i64,
    pub primitives_source: JsonValue,
}

/// 같은 `(wallet, protocol, position_id, chain)` 이 있으면 UPSERT (data + summary 업데이트).
pub fn upsert(tx: &Transaction<'_>, p: &PositionInsert) -> DbResult<i64> {
    let data_s = serde_json::to_string(&p.data)?;
    let source_s = serde_json::to_string(&p.primitives_source)?;
    tx.execute(
        "INSERT INTO positions \
           (wallet_id, position_id, protocol, chain, kind, market, summary, data_json, \
            primitives_synced_at, primitives_source_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(wallet_id, protocol, position_id, chain) DO UPDATE SET \
           kind = excluded.kind, \
           market = excluded.market, \
           summary = excluded.summary, \
           data_json = excluded.data_json, \
           primitives_synced_at = excluded.primitives_synced_at, \
           primitives_source_json = excluded.primitives_source_json",
        params![
            p.wallet_id,
            p.position_id,
            p.protocol,
            p.chain,
            p.kind,
            p.market,
            p.summary,
            data_s,
            p.primitives_synced_at,
            source_s,
        ],
    )?;
    // 동일 id 가져오기 (UPSERT 후).
    let id: i64 = tx.query_row(
        "SELECT id FROM positions WHERE wallet_id=?1 AND protocol=?2 AND position_id=?3 \
         AND IFNULL(chain,'') = IFNULL(?4,'')",
        params![p.wallet_id, p.protocol, p.position_id, p.chain],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub fn delete(tx: &Transaction<'_>, id: i64) -> DbResult<bool> {
    let n = tx.execute("DELETE FROM positions WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

pub fn list_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<PositionRow>> {
    let mut stmt = tx.prepare(
        "SELECT id, wallet_id, position_id, protocol, chain, kind, market, summary, \
                data_json, primitives_synced_at, primitives_source_json \
         FROM positions WHERE wallet_id = ?1 \
         ORDER BY kind, protocol, market",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| {
            Ok(PositionRow {
                id: r.get(0)?,
                wallet_id: r.get(1)?,
                position_id: r.get(2)?,
                protocol: r.get(3)?,
                chain: r.get(4)?,
                kind: r.get(5)?,
                market: r.get(6)?,
                summary: r.get(7)?,
                data_json: r.get(8)?,
                primitives_synced_at: r.get(9)?,
                primitives_source_json: r.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// count by kind — UI summary.
pub fn count_by_kind(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<(String, i64)>> {
    let mut stmt = tx.prepare(
        "SELECT kind, COUNT(*) FROM positions WHERE wallet_id = ?1 GROUP BY kind",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
    fn upsert_and_list_two_kinds() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            // Aave lending
            upsert(
                tx,
                &PositionInsert {
                    wallet_id: w,
                    position_id: "aave-v3/usdc".into(),
                    protocol: "aave-v3".into(),
                    chain: Some("eip155:1".into()),
                    kind: "lending_account".into(),
                    market: Some("USDC".into()),
                    summary: Some("supplied 2000, borrowed 500, HF 1.92".into()),
                    data: json!({"supplied":"2000000000","borrowed":"500000000","hf":"1.92"}),
                    primitives_synced_at: 1_738_000_000,
                    primitives_source: json!({"kind":"user_supplied"}),
                },
            )?;
            // Hyperliquid perp
            upsert(
                tx,
                &PositionInsert {
                    wallet_id: w,
                    position_id: "hl/ETH-USD/long".into(),
                    protocol: "hyperliquid".into(),
                    chain: None,
                    kind: "perp_position".into(),
                    market: Some("ETH-USD".into()),
                    summary: Some("long 2 ETH @ $2004, 5x, PnL +$12".into()),
                    data: json!({"side":"long","size":"2","entry":"2004","leverage":5}),
                    primitives_synced_at: 1_738_000_000,
                    primitives_source: json!({"kind":"venue_api"}),
                },
            )?;
            let rows = list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 2);
            // ORDER BY kind, protocol, market — lending < perp_*
            assert_eq!(rows[0].kind, "lending_account");
            assert_eq!(rows[1].kind, "perp_position");

            let counts = count_by_kind(tx, w)?;
            assert_eq!(counts.len(), 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn upsert_updates_existing() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            let p1 = PositionInsert {
                wallet_id: w,
                position_id: "p".into(),
                protocol: "aave".into(),
                chain: Some("eip155:1".into()),
                kind: "lending_account".into(),
                market: None,
                summary: Some("first".into()),
                data: json!({"v":1}),
                primitives_synced_at: 1,
                primitives_source: json!({}),
            };
            let id1 = upsert(tx, &p1)?;
            let mut p2 = p1.clone();
            p2.summary = Some("second".into());
            p2.data = json!({"v":2});
            let id2 = upsert(tx, &p2)?;
            assert_eq!(id1, id2);

            let rows = list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].summary.as_deref(), Some("second"));
            Ok(())
        })
        .unwrap();
    }
}
