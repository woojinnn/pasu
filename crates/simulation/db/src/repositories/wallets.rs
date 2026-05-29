//! `wallets` + `wallet_chains` 테이블 CRUD.

use rusqlite::{Transaction, params};

use simulation_state::primitives::ChainId;

use crate::error::DbResult;

/// 새 wallet 을 INSERT 할 때의 입력.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletInsert {
    /// 소문자 0x 주소.
    pub address: String,
    pub label: Option<String>,
    pub is_owned: bool,
    pub created_at: i64,
    pub chains: Vec<ChainId>,
}

/// SELECT 결과로 돌아오는 wallet row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wallet {
    pub id: i64,
    pub address: String,
    pub label: Option<String>,
    pub is_owned: bool,
    pub created_at: i64,
    pub archived_at: Option<i64>,
    pub chains: Vec<ChainId>,
}

/// 새 wallet INSERT — 같은 주소가 이미 있으면 UNIQUE 위반 에러.
pub fn insert(tx: &Transaction<'_>, w: &WalletInsert) -> DbResult<i64> {
    tx.execute(
        "INSERT INTO wallets (address, label, is_owned, created_at) \
         VALUES (?1, ?2, ?3, ?4)",
        params![w.address.to_lowercase(), w.label, i64::from(w.is_owned), w.created_at],
    )?;
    let id = tx.last_insert_rowid();

    for chain in &w.chains {
        tx.execute(
            "INSERT INTO wallet_chains (wallet_id, chain) VALUES (?1, ?2)",
            params![id, chain.to_string()],
        )?;
    }
    Ok(id)
}

/// id 로 한 wallet 가져옴 (chains 포함).
pub fn get_by_id(tx: &Transaction<'_>, id: i64) -> DbResult<Option<Wallet>> {
    let row = tx
        .prepare(
            "SELECT id, address, label, is_owned, created_at, archived_at \
             FROM wallets WHERE id = ?1",
        )?
        .query_row(params![id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        });

    let (id, address, label, is_owned, created_at, archived_at) = match row {
        Ok(t) => t,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let chains = list_chains(tx, id)?;
    Ok(Some(Wallet {
        id,
        address,
        label,
        is_owned: is_owned != 0,
        created_at,
        archived_at,
        chains,
    }))
}

/// 주소로 wallet 가져옴.
pub fn get_by_address(tx: &Transaction<'_>, address: &str) -> DbResult<Option<Wallet>> {
    let row = tx
        .prepare(
            "SELECT id, address, label, is_owned, created_at, archived_at \
             FROM wallets WHERE address = ?1",
        )?
        .query_row(params![address.to_lowercase()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        });

    let (id, address, label, is_owned, created_at, archived_at) = match row {
        Ok(t) => t,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let chains = list_chains(tx, id)?;
    Ok(Some(Wallet {
        id,
        address,
        label,
        is_owned: is_owned != 0,
        created_at,
        archived_at,
        chains,
    }))
}

/// 모든 wallet (archived 제외).
pub fn list_active(tx: &Transaction<'_>) -> DbResult<Vec<Wallet>> {
    let mut stmt = tx.prepare(
        "SELECT id, address, label, is_owned, created_at, archived_at \
         FROM wallets WHERE archived_at IS NULL ORDER BY created_at ASC",
    )?;
    let rows: Vec<_> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for (id, address, label, is_owned, created_at, archived_at) in rows {
        let chains = list_chains(tx, id)?;
        out.push(Wallet {
            id,
            address,
            label,
            is_owned: is_owned != 0,
            created_at,
            archived_at,
            chains,
        });
    }
    Ok(out)
}

/// 한 wallet 의 추적 chain 들.
pub fn list_chains(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<ChainId>> {
    let mut stmt = tx.prepare(
        "SELECT chain FROM wallet_chains WHERE wallet_id = ?1 ORDER BY chain",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| r.get::<_, String>(0))?
        .map(|r| r.map(ChainId::from))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// soft delete — archived_at 만 채움.
pub fn archive(tx: &Transaction<'_>, wallet_id: i64, at: i64) -> DbResult<bool> {
    let n = tx.execute(
        "UPDATE wallets SET archived_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
        params![wallet_id, at],
    )?;
    Ok(n > 0)
}

/// hard delete — wallet + 모든 의존 row (CASCADE) 삭제.
pub fn delete(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<bool> {
    let n = tx.execute("DELETE FROM wallets WHERE id = ?1", params![wallet_id])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::Pool;

    fn fresh_pool() -> Pool {
        let pool = Pool::open_in_memory();
        crate::run_migrations(&pool).unwrap();
        pool
    }

    fn sample(addr: &str) -> WalletInsert {
        WalletInsert {
            address: addr.into(),
            label: Some("main".into()),
            is_owned: true,
            created_at: 1_700_000_000,
            chains: vec![ChainId::ethereum_mainnet(), ChainId::new("eip155:42161")],
        }
    }

    #[test]
    fn insert_and_get() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let id = insert(tx, &sample("0xAlice")).unwrap();
            assert!(id >= 1);
            let w = get_by_id(tx, id).unwrap().unwrap();
            assert_eq!(w.address, "0xalice"); // 소문자 정규화
            assert_eq!(w.chains.len(), 2);
            assert!(w.is_owned);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn get_by_address_normalizes_case() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            insert(tx, &sample("0xBob")).unwrap();
            let w = get_by_address(tx, "0xBOB").unwrap().unwrap();
            assert_eq!(w.address, "0xbob");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_active_skips_archived() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let a = insert(tx, &sample("0xA")).unwrap();
            let _b = insert(tx, &sample("0xB")).unwrap();
            archive(tx, a, 1_700_000_500).unwrap();
            let active = list_active(tx).unwrap();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].address, "0xb");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn duplicate_address_errors() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            insert(tx, &sample("0xDup")).unwrap();
            let err = insert(tx, &sample("0xDUP")).unwrap_err();
            assert!(format!("{err}").to_lowercase().contains("unique"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn delete_cascades_chains() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let id = insert(tx, &sample("0xDel")).unwrap();
            assert!(delete(tx, id).unwrap());
            assert!(list_chains(tx, id).unwrap().is_empty());
            Ok(())
        })
        .unwrap();
    }
}
