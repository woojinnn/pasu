//! `block_heights` table CRUD.

use rusqlite::{params, Transaction};

use simulation_state::{BlockHeight, ChainId};

use crate::error::{DbError, DbResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeightRow {
    pub wallet_id: i64,
    pub chain: ChainId,
    pub height: BlockHeight,
}

pub fn upsert(
    tx: &Transaction<'_>,
    wallet_id: i64,
    chain: &ChainId,
    height: &BlockHeight,
) -> DbResult<()> {
    let number = i64::try_from(height.number)
        .map_err(|_| DbError::Invariant("block height overflow".into()))?;
    let observed_at = i64::try_from(height.time)
        .map_err(|_| DbError::Invariant("block observed_at overflow".into()))?;
    tx.execute(
        "INSERT INTO block_heights (wallet_id, chain, height, observed_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(wallet_id, chain) DO UPDATE SET \
           height = excluded.height, observed_at = excluded.observed_at",
        params![wallet_id, chain.to_string(), number, observed_at],
    )?;
    Ok(())
}

pub fn list_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<BlockHeightRow>> {
    let mut stmt = tx.prepare(
        "SELECT wallet_id, chain, height, observed_at \
         FROM block_heights WHERE wallet_id = ?1 ORDER BY chain",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| {
            let height = r.get::<_, i64>(2)?;
            let observed_at = r.get::<_, i64>(3)?;
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                height,
                observed_at,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    rows.into_iter()
        .map(|(wallet_id, chain, height, observed_at)| {
            let number = u64::try_from(height)
                .map_err(|_| DbError::Invariant("block height negative".into()))?;
            let time = u64::try_from(observed_at)
                .map_err(|_| DbError::Invariant("block observed_at negative".into()))?;
            Ok(BlockHeightRow {
                wallet_id,
                chain: ChainId::from(chain),
                height: BlockHeight { number, time },
            })
        })
        .collect()
}

pub fn delete_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<usize> {
    let n = tx.execute(
        "DELETE FROM block_heights WHERE wallet_id = ?1",
        params![wallet_id],
    )?;
    Ok(n)
}
