//! `user_policies` CRUD — 사용자 Cedar 정책 저장소.

use rusqlite::{params, Transaction};

use crate::error::DbResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserPolicyRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub cedar_text: String,
    pub severity: String, // "deny" | "warn" | "info"
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct UserPolicyInsert {
    pub name: String,
    pub description: Option<String>,
    pub cedar_text: String,
    pub severity: String,
}

pub fn insert(tx: &Transaction<'_>, p: &UserPolicyInsert, now: i64) -> DbResult<i64> {
    tx.execute(
        "INSERT INTO user_policies (name, description, cedar_text, severity, enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![p.name, p.description, p.cedar_text, p.severity, now],
    )?;
    Ok(tx.last_insert_rowid())
}

pub fn delete(tx: &Transaction<'_>, id: i64) -> DbResult<bool> {
    let n = tx.execute("DELETE FROM user_policies WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

pub fn set_enabled(tx: &Transaction<'_>, id: i64, enabled: bool, now: i64) -> DbResult<()> {
    tx.execute(
        "UPDATE user_policies SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, i64::from(enabled), now],
    )?;
    Ok(())
}

/// Patch a policy. Only `Some` fields are written; nested `Option` on
/// `description` lets a caller distinguish "unchanged" from "set to NULL".
#[derive(Clone, Debug, Default)]
pub struct UserPolicyPatch {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub cedar_text: Option<String>,
    pub severity: Option<String>,
    pub enabled: Option<bool>,
}

impl UserPolicyPatch {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.cedar_text.is_none()
            && self.severity.is_none()
            && self.enabled.is_none()
    }
}

pub fn update(tx: &Transaction<'_>, id: i64, patch: &UserPolicyPatch, now: i64) -> DbResult<bool> {
    if patch.is_empty() {
        return Ok(false);
    }
    let mut total = 0usize;
    if let Some(name) = &patch.name {
        total += tx.execute(
            "UPDATE user_policies SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, name, now],
        )?;
    }
    if let Some(desc) = &patch.description {
        total += tx.execute(
            "UPDATE user_policies SET description = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, desc, now],
        )?;
    }
    if let Some(cedar) = &patch.cedar_text {
        total += tx.execute(
            "UPDATE user_policies SET cedar_text = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, cedar, now],
        )?;
    }
    if let Some(severity) = &patch.severity {
        total += tx.execute(
            "UPDATE user_policies SET severity = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, severity, now],
        )?;
    }
    if let Some(enabled) = patch.enabled {
        total += tx.execute(
            "UPDATE user_policies SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, i64::from(enabled), now],
        )?;
    }
    Ok(total > 0)
}

pub fn get(tx: &Transaction<'_>, id: i64) -> DbResult<Option<UserPolicyRow>> {
    let row = tx
        .prepare(
            "SELECT id, name, description, cedar_text, severity, enabled, created_at, updated_at \
             FROM user_policies WHERE id = ?1",
        )?
        .query_row(params![id], |r| {
            Ok(UserPolicyRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                cedar_text: r.get(3)?,
                severity: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        });
    match row {
        Ok(r) => Ok(Some(r)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn list_all(tx: &Transaction<'_>) -> DbResult<Vec<UserPolicyRow>> {
    let mut stmt = tx.prepare(
        "SELECT id, name, description, cedar_text, severity, enabled, created_at, updated_at \
         FROM user_policies ORDER BY enabled DESC, created_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UserPolicyRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                cedar_text: r.get(3)?,
                severity: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_enabled(tx: &Transaction<'_>) -> DbResult<Vec<UserPolicyRow>> {
    let mut stmt = tx.prepare(
        "SELECT id, name, description, cedar_text, severity, enabled, created_at, updated_at \
         FROM user_policies WHERE enabled = 1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UserPolicyRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                cedar_text: r.get(3)?,
                severity: r.get(4)?,
                enabled: true,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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

    #[test]
    fn insert_and_list() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let id = insert(
                tx,
                &UserPolicyInsert {
                    name: "max HF".into(),
                    description: Some("borrow 후 HF < 1.5 차단".into()),
                    cedar_text: r#"forbid(principal, action == Action::"Borrow", resource) when { context.outcome.hf < decimal("1.5") };"#.into(),
                    severity: "deny".into(),
                },
                1_700_000_000,
            )?;
            assert!(id >= 1);
            let rows = list_all(tx)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, "max HF");
            assert!(rows[0].enabled);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn enable_disable_filters_list_enabled() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let id1 = insert(
                tx,
                &UserPolicyInsert {
                    name: "a".into(),
                    description: None,
                    cedar_text: "permit(principal, action, resource);".into(),
                    severity: "info".into(),
                },
                1,
            )?;
            let _id2 = insert(
                tx,
                &UserPolicyInsert {
                    name: "b".into(),
                    description: None,
                    cedar_text: "permit(principal, action, resource);".into(),
                    severity: "info".into(),
                },
                2,
            )?;
            set_enabled(tx, id1, false, 100)?;
            let all = list_all(tx)?;
            assert_eq!(all.len(), 2);
            let enabled = list_enabled(tx)?;
            assert_eq!(enabled.len(), 1);
            assert_eq!(enabled[0].name, "b");
            Ok(())
        })
        .unwrap();
    }
}
