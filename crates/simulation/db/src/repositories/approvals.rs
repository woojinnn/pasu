//! `approvals_erc20` / `approvals_set_for_all` / `approvals_permit2` CRUD.
//!
//! 세 종류가 같이 살지만 데이터 모양이 다 다르므로 3개 sub-module 로 분리.
//! 각각 (`wallet_id`, chain, contract, [spender|operator]) 복합키 sparse 테이블.

use rusqlite::{params, Transaction};

use crate::error::DbResult;

// ---------------------------------------------------------------------------
// ERC20
// ---------------------------------------------------------------------------

/// ERC20 approval 한 row (DB 표현).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Erc20ApprovalRow {
    pub wallet_id: i64,
    /// CAIP-2.
    pub chain: String,
    /// 0x... (소문자).
    pub token_address: String,
    /// 0x... (소문자).
    pub spender: String,
    /// U256 decimal string.
    pub amount: String,
    pub is_unlimited: bool,
    pub last_set_at: i64,
}

pub mod erc20 {
    use super::Erc20ApprovalRow;
    use rusqlite::{params, Transaction};

    use crate::error::DbResult;

    pub fn upsert(tx: &Transaction<'_>, row: &Erc20ApprovalRow) -> DbResult<()> {
        tx.execute(
            "INSERT INTO approvals_erc20 \
               (wallet_id, chain, token_address, spender, amount, is_unlimited, last_set_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(wallet_id, chain, token_address, spender) DO UPDATE SET \
               amount = excluded.amount, \
               is_unlimited = excluded.is_unlimited, \
               last_set_at = excluded.last_set_at",
            params![
                row.wallet_id,
                row.chain.to_lowercase(),
                row.token_address.to_lowercase(),
                row.spender.to_lowercase(),
                row.amount,
                i64::from(row.is_unlimited),
                row.last_set_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete(
        tx: &Transaction<'_>,
        wallet_id: i64,
        chain: &str,
        token: &str,
        spender: &str,
    ) -> DbResult<bool> {
        let n = tx.execute(
            "DELETE FROM approvals_erc20 WHERE wallet_id = ?1 AND chain = ?2 \
             AND token_address = ?3 AND spender = ?4",
            params![
                wallet_id,
                chain.to_lowercase(),
                token.to_lowercase(),
                spender.to_lowercase()
            ],
        )?;
        Ok(n > 0)
    }

    pub fn list_for_wallet(
        tx: &Transaction<'_>,
        wallet_id: i64,
    ) -> DbResult<Vec<Erc20ApprovalRow>> {
        let mut stmt = tx.prepare(
            "SELECT wallet_id, chain, token_address, spender, amount, is_unlimited, last_set_at \
             FROM approvals_erc20 WHERE wallet_id = ?1 \
             ORDER BY is_unlimited DESC, last_set_at DESC",
        )?;
        let rows = stmt
            .query_map(params![wallet_id], |r| {
                Ok(Erc20ApprovalRow {
                    wallet_id: r.get(0)?,
                    chain: r.get(1)?,
                    token_address: r.get(2)?,
                    spender: r.get(3)?,
                    amount: r.get(4)?,
                    is_unlimited: r.get::<_, i64>(5)? != 0,
                    last_set_at: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Total ERC-20 + setForAll grants the wallet has where the allowance
    /// is the U256 max. Drives the dashboard "unlimited approvals" badge.
    pub fn count_unlimited_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<i64> {
        let erc20: i64 = tx.query_row(
            "SELECT COUNT(*) FROM approvals_erc20 WHERE wallet_id = ?1 AND is_unlimited = 1",
            params![wallet_id],
            |r| r.get(0),
        )?;
        let sfa: i64 = tx.query_row(
            "SELECT COUNT(*) FROM approvals_set_for_all WHERE wallet_id = ?1",
            params![wallet_id],
            |r| r.get(0),
        )?;
        Ok(erc20 + sfa)
    }
}

// ---------------------------------------------------------------------------
// setApprovalForAll (ERC721 / ERC1155)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetForAllRow {
    pub wallet_id: i64,
    pub chain: String,
    pub collection: String,
    pub operator: String,
    pub set_at: Option<i64>,
}

pub mod set_for_all {
    use super::SetForAllRow;
    use rusqlite::{params, Transaction};

    use crate::error::DbResult;

    pub fn upsert(tx: &Transaction<'_>, row: &SetForAllRow) -> DbResult<()> {
        tx.execute(
            "INSERT INTO approvals_set_for_all (wallet_id, chain, collection, operator, set_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(wallet_id, chain, collection, operator) DO UPDATE SET \
               set_at = excluded.set_at",
            params![
                row.wallet_id,
                row.chain.to_lowercase(),
                row.collection.to_lowercase(),
                row.operator.to_lowercase(),
                row.set_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete(
        tx: &Transaction<'_>,
        wallet_id: i64,
        chain: &str,
        collection: &str,
        operator: &str,
    ) -> DbResult<bool> {
        let n = tx.execute(
            "DELETE FROM approvals_set_for_all WHERE wallet_id = ?1 AND chain = ?2 \
             AND collection = ?3 AND operator = ?4",
            params![
                wallet_id,
                chain.to_lowercase(),
                collection.to_lowercase(),
                operator.to_lowercase(),
            ],
        )?;
        Ok(n > 0)
    }

    pub fn list_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<SetForAllRow>> {
        let mut stmt = tx.prepare(
            "SELECT wallet_id, chain, collection, operator, set_at \
             FROM approvals_set_for_all WHERE wallet_id = ?1 \
             ORDER BY set_at DESC NULLS LAST",
        )?;
        let rows = stmt
            .query_map(params![wallet_id], |r| {
                Ok(SetForAllRow {
                    wallet_id: r.get(0)?,
                    chain: r.get(1)?,
                    collection: r.get(2)?,
                    operator: r.get(3)?,
                    set_at: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// ---------------------------------------------------------------------------
// Permit2
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permit2Row {
    pub wallet_id: i64,
    pub chain: String,
    pub token_address: String,
    pub spender: String,
    pub amount: String,
    pub expiration: i64,
    pub nonce: i64,
}

pub mod permit2 {
    use super::Permit2Row;
    use rusqlite::{params, Transaction};

    use crate::error::DbResult;

    pub fn upsert(tx: &Transaction<'_>, row: &Permit2Row) -> DbResult<()> {
        tx.execute(
            "INSERT INTO approvals_permit2 \
               (wallet_id, chain, token_address, spender, amount, expiration, nonce) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(wallet_id, chain, token_address, spender) DO UPDATE SET \
               amount = excluded.amount, \
               expiration = excluded.expiration, \
               nonce = excluded.nonce",
            params![
                row.wallet_id,
                row.chain.to_lowercase(),
                row.token_address.to_lowercase(),
                row.spender.to_lowercase(),
                row.amount,
                row.expiration,
                row.nonce,
            ],
        )?;
        Ok(())
    }

    pub fn delete(
        tx: &Transaction<'_>,
        wallet_id: i64,
        chain: &str,
        token: &str,
        spender: &str,
    ) -> DbResult<bool> {
        let n = tx.execute(
            "DELETE FROM approvals_permit2 WHERE wallet_id = ?1 AND chain = ?2 \
             AND token_address = ?3 AND spender = ?4",
            params![
                wallet_id,
                chain.to_lowercase(),
                token.to_lowercase(),
                spender.to_lowercase()
            ],
        )?;
        Ok(n > 0)
    }

    pub fn list_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<Permit2Row>> {
        let mut stmt = tx.prepare(
            "SELECT wallet_id, chain, token_address, spender, amount, expiration, nonce \
             FROM approvals_permit2 WHERE wallet_id = ?1 \
             ORDER BY expiration DESC",
        )?;
        let rows = stmt
            .query_map(params![wallet_id], |r| {
                Ok(Permit2Row {
                    wallet_id: r.get(0)?,
                    chain: r.get(1)?,
                    token_address: r.get(2)?,
                    spender: r.get(3)?,
                    amount: r.get(4)?,
                    expiration: r.get(5)?,
                    nonce: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Wallet 의 모든 approval 종류 합쳐 한 번에 가져오기 — UI 편의용.
pub fn list_all_for_wallet(
    tx: &Transaction<'_>,
    wallet_id: i64,
) -> DbResult<(Vec<Erc20ApprovalRow>, Vec<SetForAllRow>, Vec<Permit2Row>)> {
    let e = erc20::list_for_wallet(tx, wallet_id)?;
    let s = set_for_all::list_for_wallet(tx, wallet_id)?;
    let p = permit2::list_for_wallet(tx, wallet_id)?;
    Ok((e, s, p))
}

// 사용 안 함 silence.
#[allow(dead_code)]
fn _unused(_: &Transaction<'_>) {
    let _ = params![1];
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

    #[test]
    fn erc20_round_trip() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            erc20::upsert(
                tx,
                &Erc20ApprovalRow {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    token_address: "0xUSDC".into(),
                    spender: "0xUniswapRouter".into(),
                    amount: "1000000000000".into(),
                    is_unlimited: false,
                    last_set_at: 1_738_000_000,
                },
            )?;
            erc20::upsert(
                tx,
                &Erc20ApprovalRow {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    token_address: "0xUSDT".into(),
                    spender: "0xAavePool".into(),
                    amount: "115792089237316195423570985008687907853269984665640564039457584007913129639935".into(),
                    is_unlimited: true,
                    last_set_at: 1_738_500_000,
                },
            )?;
            let rows = erc20::list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 2);
            // is_unlimited DESC → unlimited first.
            assert!(rows[0].is_unlimited);
            assert_eq!(rows[0].token_address, "0xusdt");

            // delete
            assert!(erc20::delete(tx, w, "eip155:1", "0xUSDT", "0xAavePool")?);
            let rows = erc20::list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn set_for_all_round_trip() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            set_for_all::upsert(
                tx,
                &SetForAllRow {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    collection: "0xBAYC".into(),
                    operator: "0xOpenSea".into(),
                    set_at: Some(1_738_000_000),
                },
            )?;
            let rows = set_for_all::list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].collection, "0xbayc");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn permit2_round_trip() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            permit2::upsert(
                tx,
                &Permit2Row {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    token_address: "0xUSDC".into(),
                    spender: "0xUniversalRouter".into(),
                    amount: "5000000000".into(),
                    expiration: 1_900_000_000,
                    nonce: 7,
                },
            )?;
            let rows = permit2::list_for_wallet(tx, w)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].nonce, 7);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_all_combines_three_kinds() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let w = ins_wallet(tx, "0xowner");
            erc20::upsert(
                tx,
                &Erc20ApprovalRow {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    token_address: "0xT".into(),
                    spender: "0xS".into(),
                    amount: "1".into(),
                    is_unlimited: false,
                    last_set_at: 1,
                },
            )?;
            set_for_all::upsert(
                tx,
                &SetForAllRow {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    collection: "0xC".into(),
                    operator: "0xO".into(),
                    set_at: Some(1),
                },
            )?;
            permit2::upsert(
                tx,
                &Permit2Row {
                    wallet_id: w,
                    chain: "eip155:1".into(),
                    token_address: "0xT".into(),
                    spender: "0xS".into(),
                    amount: "2".into(),
                    expiration: 100,
                    nonce: 0,
                },
            )?;

            let (e, s, p) = list_all_for_wallet(tx, w)?;
            assert_eq!(e.len(), 1);
            assert_eq!(s.len(), 1);
            assert_eq!(p.len(), 1);
            Ok(())
        })
        .unwrap();
    }
}
