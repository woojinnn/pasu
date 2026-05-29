//! SQLite Connection wrapper.
//!
//! Phase 1 에서는 진짜 connection pool 이 아니라 `Arc<Mutex<Connection>>` 한 개.
//! 사용자당 1 DB 라 동시 writer 가 1명이고, SQLite WAL mode 가 다중 reader 를
//! lock 없이 처리하므로 단일 connection 으로 충분하다.
//!
//! `Pool::open` 시:
//! 1. 부모 디렉토리 자동 생성
//! 2. WAL mode 활성화
//! 3. foreign_keys ON
//! 4. busy_timeout 5초 (다른 process 가 잡고 있어도 잠시 대기)

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, Transaction};

use crate::error::DbResult;

/// 한 사용자의 DB 파일을 가리키는 핸들.
///
/// Clone 가능 — 같은 connection 을 여러 호출자가 공유할 때 사용.
/// (실제 락은 `with_conn` / `with_tx` 안에서만 잡힘.)
#[derive(Clone)]
pub struct Pool {
    path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("path", &self.path)
            .field("conn", &"<Connection>")
            .finish()
    }
}

impl Pool {
    /// 파일을 열고 (없으면 생성), pragma 설정.
    pub fn open(path: impl AsRef<Path>) -> DbResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    crate::error::DbError::Migration {
                        step: format!("mkdir {}", parent.display()),
                        reason: e.to_string(),
                    }
                })?;
            }
        }

        let conn = Connection::open(&path)?;
        apply_pragmas(&conn)?;
        Ok(Self {
            path,
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// In-memory 인스턴스 — 테스트 전용.
    #[must_use]
    pub fn open_in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("open_in_memory should not fail");
        apply_pragmas(&conn).expect("apply_pragmas on in-memory");
        Self {
            path: PathBuf::from(":memory:"),
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// DB 파일 경로.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read-only 접근 — closure 안에서만 Connection 사용 가능.
    pub fn with_conn<R>(
        &self,
        f: impl FnOnce(&Connection) -> DbResult<R>,
    ) -> DbResult<R> {
        let guard = self.conn.lock().expect("pool mutex poisoned");
        f(&guard)
    }

    /// `BEGIN IMMEDIATE` 트랜잭션 하나 잡고 closure 실행.
    /// closure 가 `Ok` 반환 → `COMMIT`, `Err` 반환 → `ROLLBACK`.
    pub fn with_tx<R>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> DbResult<R>,
    ) -> DbResult<R> {
        let mut guard = self.conn.lock().expect("pool mutex poisoned");
        let tx = guard.transaction()?;
        match f(&tx) {
            Ok(out) => {
                tx.commit()?;
                Ok(out)
            }
            Err(e) => {
                // rollback 은 drop 시 자동 — 명시적 호출은 에러 가능성 X
                drop(tx);
                Err(e)
            }
        }
    }
}

/// 모든 connection 에 일관 적용할 pragma 세트.
fn apply_pragmas(conn: &Connection) -> DbResult<()> {
    // WAL mode — multiple readers, single writer, no lock for reads.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // foreign key constraint 검사 ON (default OFF in SQLite).
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // 다른 process 가 잡고 있을 때 5초 대기.
    conn.pragma_update(None, "busy_timeout", 5000)?;
    // synchronous=NORMAL — WAL 과 안전 + 성능 균형.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_in_memory_pool() {
        let pool = Pool::open_in_memory();
        assert_eq!(pool.path(), Path::new(":memory:"));
    }

    #[test]
    fn opens_file_creates_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subdir").join("scopeball.db");
        assert!(!path.parent().unwrap().exists());
        let _pool = Pool::open(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn pragmas_are_set() {
        let pool = Pool::open_in_memory();
        pool.with_conn(|c| {
            let fk: i64 = c.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
            assert_eq!(fk, 1, "foreign_keys should be ON");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn tx_commits_on_ok() {
        let pool = Pool::open_in_memory();
        pool.with_conn(|c| {
            c.execute("CREATE TABLE t (v INTEGER)", []).unwrap();
            Ok(())
        })
        .unwrap();
        pool.with_tx(|tx| {
            tx.execute("INSERT INTO t VALUES (42)", []).unwrap();
            Ok(())
        })
        .unwrap();
        pool.with_conn(|c| {
            let n: i64 = c.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 42);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn tx_rolls_back_on_err() {
        let pool = Pool::open_in_memory();
        pool.with_conn(|c| {
            c.execute("CREATE TABLE t (v INTEGER)", []).unwrap();
            Ok(())
        })
        .unwrap();
        let res: DbResult<()> = pool.with_tx(|tx| {
            tx.execute("INSERT INTO t VALUES (99)", []).unwrap();
            Err(crate::error::DbError::Invariant("force rollback".into()))
        });
        assert!(res.is_err());
        pool.with_conn(|c| {
            let n: i64 = c
                .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "rollback should have removed insert");
            Ok(())
        })
        .unwrap();
    }
}
