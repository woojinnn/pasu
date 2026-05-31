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

const MIGRATION_002: Migration = Migration {
    version: 2,
    description: "approvals — erc20 / set_for_all / permit2",
    sql: include_str!("migrations/002_approvals.sql"),
};

const MIGRATION_003: Migration = Migration {
    version: 3,
    description: "positions — lending / perp / airdrop / launchpad / vesting (generic)",
    sql: include_str!("migrations/003_positions.sql"),
};

const MIGRATION_004: Migration = Migration {
    version: 4,
    description:
        "pending_txs — offchain signature ledger (UniswapX intent / Permit2 / Safe pre-sign)",
    sql: include_str!("migrations/004_pending_txs.sql"),
};

const MIGRATION_005: Migration = Migration {
    version: 5,
    description: "user_policies — Cedar policy text storage (Phase 5)",
    sql: include_str!("migrations/005_user_policies.sql"),
};

const MIGRATION_006: Migration = Migration {
    version: 6,
    description: "positions — allow hyperliquid_account kind (widen CHECK via table rebuild)",
    sql: include_str!("migrations/006_hyperliquid_account_kind.sql"),
};

const ALL_MIGRATIONS: &[Migration] = &[
    MIGRATION_001,
    MIGRATION_002,
    MIGRATION_003,
    MIGRATION_004,
    MIGRATION_005,
    MIGRATION_006,
];

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
            .query_row("SELECT MAX(version) FROM _schema_migrations", [], |r| {
                r.get(0)
            })
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
        // Phase 2: 002 까지 적용.
        assert_eq!(current_version(&pool).unwrap(), Some(6));
    }

    #[test]
    fn runs_is_idempotent() {
        let pool = Pool::open_in_memory();
        run(&pool).unwrap();
        run(&pool).unwrap(); // 두 번째 호출도 OK
        run(&pool).unwrap(); // 세 번째도
        assert_eq!(current_version(&pool).unwrap(), Some(6));

        // _schema_migrations 에는 적용된 버전 수 만큼만 row.
        pool.with_conn(|c| {
            let n: i64 = c
                .query_row("SELECT COUNT(*) FROM _schema_migrations", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 6);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn migration_006_preserves_existing_position_rows() {
        use crate::repositories::positions::{list_for_wallet, upsert, PositionInsert};
        use crate::repositories::wallets::{insert as insert_wallet, WalletInsert};
        use serde_json::json;
        use simulation_state::primitives::ChainId;

        let pool = Pool::open_in_memory();
        // run() creates _schema_migrations before its loop; apply_one does not.
        pool.with_conn(|c| {
            c.execute_batch(SCHEMA_MIGRATIONS_TABLE)?;
            Ok(())
        })
        .unwrap();
        // Apply 001..=005 only: positions exists with the OLD (pre-006) CHECK.
        for m in &[
            MIGRATION_001,
            MIGRATION_002,
            MIGRATION_003,
            MIGRATION_004,
            MIGRATION_005,
        ] {
            apply_one(&pool, m).unwrap();
        }

        // Stage one OLD-kind row with a DISTINCT value in every column.
        let (wallet_id, staged_id) = pool
            .with_tx(|tx| {
                let w = insert_wallet(
                    tx,
                    &WalletInsert {
                        address: "0xowner".into(),
                        label: None,
                        is_owned: true,
                        created_at: 1_700_000_000,
                        chains: vec![ChainId::ethereum_mainnet()],
                    },
                )?;
                let pid = upsert(
                    tx,
                    &PositionInsert {
                        wallet_id: w,
                        position_id: "POS_ID".into(),
                        protocol: "PROTO".into(),
                        chain: Some("eip155:1".into()),
                        kind: "perp_position".into(),
                        market: Some("MKT".into()),
                        summary: Some("SUM".into()),
                        data: json!({ "marker": "DATA_MARKER" }),
                        primitives_synced_at: 1_738_111_222,
                        primitives_source: json!({ "src": "SOURCE_MARKER" }),
                    },
                )?;
                Ok((w, pid))
            })
            .unwrap();

        // Migration under test: rebuild + copy.
        apply_one(&pool, &MIGRATION_006).unwrap();

        // Every field must survive the INSERT...SELECT, in the correct column.
        pool.with_tx(|tx| {
            let rows = list_for_wallet(tx, wallet_id)?;
            assert_eq!(rows.len(), 1, "row count must survive rebuild");
            let r = &rows[0];
            assert_eq!(r.id, staged_id, "PK preserved");
            assert_eq!(r.position_id, "POS_ID");
            assert_eq!(r.protocol, "PROTO");
            assert_eq!(r.chain.as_deref(), Some("eip155:1"));
            assert_eq!(r.kind, "perp_position");
            assert_eq!(r.market.as_deref(), Some("MKT"));
            assert_eq!(r.summary.as_deref(), Some("SUM"));
            assert!(r.data_json.contains("DATA_MARKER"));
            assert_eq!(r.primitives_synced_at, 1_738_111_222);
            assert!(r.primitives_source_json.contains("SOURCE_MARKER"));
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
