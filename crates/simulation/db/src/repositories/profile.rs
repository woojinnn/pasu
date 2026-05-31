//! `user_profile` 테이블 — DB 파일을 소유한 사용자 (singleton).

use rusqlite::{params, Transaction};
use serde_json::Value as JsonValue;

use crate::error::{DbError, DbResult};

/// `user_profile` 의 한 row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserProfile {
    pub user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    /// 사용자 설정 (retention 등). 자유 JSON.
    pub settings: JsonValue,
    pub created_at: i64,
}

/// 새 `user_profile` 을 INSERT. 이미 있으면 [`DbError::Invariant`].
pub fn insert(tx: &Transaction<'_>, p: &UserProfile) -> DbResult<()> {
    let settings_str = serde_json::to_string(&p.settings)?;
    let res = tx.execute(
        "INSERT INTO user_profile (id, user_id, email, display_name, settings_json, created_at) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![
            p.user_id,
            p.email,
            p.display_name,
            settings_str,
            p.created_at
        ],
    );
    match res {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(e, msg))
            if matches!(e.code, rusqlite::ErrorCode::ConstraintViolation) =>
        {
            Err(DbError::Invariant(format!(
                "user_profile already exists: {}",
                msg.unwrap_or_else(|| "no detail".into())
            )))
        }
        Err(e) => Err(e.into()),
    }
}

/// `user_profile` 이 있으면 UPDATE, 없으면 INSERT.
pub fn upsert(tx: &Transaction<'_>, p: &UserProfile) -> DbResult<()> {
    let settings_str = serde_json::to_string(&p.settings)?;
    tx.execute(
        "INSERT INTO user_profile (id, user_id, email, display_name, settings_json, created_at) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(id) DO UPDATE SET \
           user_id = excluded.user_id, \
           email = excluded.email, \
           display_name = excluded.display_name, \
           settings_json = excluded.settings_json",
        params![
            p.user_id,
            p.email,
            p.display_name,
            settings_str,
            p.created_at
        ],
    )?;
    Ok(())
}

/// 단일 `user_profile` row 를 가져옴. 없으면 None.
pub fn get(tx: &Transaction<'_>) -> DbResult<Option<UserProfile>> {
    let mut stmt = tx.prepare(
        "SELECT user_id, email, display_name, settings_json, created_at \
         FROM user_profile WHERE id = 1",
    )?;
    let row = stmt.query_row([], |r| {
        let settings_str: String = r.get(3)?;
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            settings_str,
            r.get::<_, i64>(4)?,
        ))
    });
    match row {
        Ok((user_id, email, display_name, settings_str, created_at)) => Ok(Some(UserProfile {
            user_id,
            email,
            display_name,
            settings: serde_json::from_str(&settings_str)?,
            created_at,
        })),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
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

    fn sample() -> UserProfile {
        UserProfile {
            user_id: "github:alice".into(),
            email: Some("alice@example.com".into()),
            display_name: Some("Alice".into()),
            settings: serde_json::json!({"retention_days": 90}),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn insert_then_get() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            insert(tx, &sample()).unwrap();
            let p = get(tx).unwrap().unwrap();
            assert_eq!(p.user_id, "github:alice");
            assert_eq!(p.settings["retention_days"], 90);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn insert_twice_errors() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            insert(tx, &sample()).unwrap();
            let err = insert(tx, &sample()).unwrap_err();
            assert!(format!("{err}").contains("already exists"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn upsert_replaces() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            upsert(tx, &sample()).unwrap();
            let mut p2 = sample();
            p2.display_name = Some("Alice Smith".into());
            upsert(tx, &p2).unwrap();
            let got = get(tx).unwrap().unwrap();
            assert_eq!(got.display_name, Some("Alice Smith".into()));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn get_empty_returns_none() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            assert!(get(tx).unwrap().is_none());
            Ok(())
        })
        .unwrap();
    }
}
