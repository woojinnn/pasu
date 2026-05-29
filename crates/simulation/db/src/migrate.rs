//! Migration runner.
//!
//! Phase 1 의 단일 migration (`MIGRATION_001`) 만 정의. 후속 schema 진화 시
//! 같은 패턴으로 `MIGRATION_002`, `MIGRATION_003` ... 늘리면 된다.
//!
//! 적용 상태는 `_schema_migrations` 테이블에 기록되어 멱등 — 같은 pool 에 두 번
//! 호출해도 두 번 적용되지 않는다.

use crate::error::{DbError, DbResult};
use crate::pool::Pool;

const SCHEMA_MIGRATIONS_TABLE: &str = r"
CREATE TABLE IF NOT EXISTS _schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at INTEGER NOT NULL,
  description TEXT NOT NULL
);
";

/// 한 migration 의 메타.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub sql: &'static str,
}

const MIGRATION_001: Migration = Migration {
    version: 1,
    description: "initial schema — profile, wallets, tokens, holdings, block_heights, state_deltas",
    sql: include_str!("migrations/001_initial.sql"),
};

const ALL_MIGRATIONS: &[Migration] = &[MIGRATION_001];

/// 모든 migration 을 멱등하게 적용. 이미 적용된 버전은 skip.
pub fn run(pool: &Pool) -> DbResult<()> {
    pool.with_conn(|c| {
        c.execute_batch(SCHEMA_MIGRATIONS_TABLE)?;
        Ok(())
    })?;

    for m in ALL_MIGRATIONS {
        apply_one(pool, m)?;
    }
    Ok(())
}

fn apply_one(pool: &Pool, m: &Migration) -> DbResult<()> {
    // 이미 적용됐는지 체크.
    let already: bool = pool.with_conn(|c| {
        let n: i64 = c.query_row(
            "SELECT COUNT(*) FROM _schema_migrations WHERE version = ?1",
            [m.version],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    })?;
    if already {
        return Ok(());
    }

    pool.with_tx(|tx| {
        tx.execute_batch(m.sql).map_err(|e| DbError::Migration {
            step: format!("migration_{:03}", m.version),
            reason: e.to_string(),
        })?;
        tx.execute(
            "INSERT INTO _schema_migrations (version, applied_at, description) \
             VALUES (?1, strftime('%s','now'), ?2)",
            (m.version, m.description),
        )?;
        Ok(())
    })?;
    Ok(())
}

/// 현재 적용된 최신 migration version. 아무것도 적용 안 됐으면 `None`.
pub fn current_version(pool: &Pool) -> DbResult<Option<i64>> {
    pool.with_conn(|c| {
        // 테이블이 없을 수도 있어서 sqlite_master 먼저 확인.
        let exists: bool = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='_schema_migrations'",
                [],
                |r| {
                    let n: i64 = r.get(0)?;
                    Ok(n > 0)
                },
            )
            .unwrap_or(false);
        if !exists {
            return Ok(None);
        }
        let v: Option<i64> = c
            .query_row(
                "SELECT MAX(version) FROM _schema_migrations",
                [],
                |r| r.get(0),
            )
            .ok();
        Ok(v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_on_fresh_db() {
        let pool = Pool::open_in_memory();
        assert_eq!(current_version(&pool).unwrap(), None);
        run(&pool).unwrap();
        assert_eq!(current_version(&pool).unwrap(), Some(1));
    }

    #[test]
    fn runs_is_idempotent() {
        let pool = Pool::open_in_memory();
        run(&pool).unwrap();
        run(&pool).unwrap(); // 두 번째 호출도 OK
        run(&pool).unwrap(); // 세 번째도
        assert_eq!(current_version(&pool).unwrap(), Some(1));

        // _schema_migrations 에는 version=1 row 1개만.
        pool.with_conn(|c| {
            let n: i64 = c
                .query_row("SELECT COUNT(*) FROM _schema_migrations", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn creates_expected_tables() {
        let pool = Pool::open_in_memory();
        run(&pool).unwrap();
        let expected = [
            "user_profile",
            "wallets",
            "wallet_chains",
            "tokens",
            "token_holdings",
            "block_heights",
            "state_deltas",
        ];
        pool.with_conn(|c| {
            for table in expected {
                let n: i64 = c
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master \
                         WHERE type='table' AND name=?1",
                        [table],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(n, 1, "table {table} should exist");
            }
            Ok(())
        })
        .unwrap();
    }
}
