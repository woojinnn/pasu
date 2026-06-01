//! `verdicts` table CRUD — Cedar policy audit log.
//!
//! Append-mostly. The only mutable column is `user_decision` (set when a
//! user reviews a `warn` row and picks trusted/cancelled). Reads support
//! the Audit/History/Findings pages via filtered + cursor-paginated
//! queries.

use rusqlite::{params, Transaction};

use crate::error::DbResult;

/// Row inserted by `POST /verdicts` (extension submits) or by the
/// future server-side cedar evaluator.
#[derive(Clone, Debug)]
pub struct VerdictInsert {
    pub delta_id: Option<i64>,
    pub wallet_id: i64,
    pub policy_id: Option<i64>,
    pub severity: String, // "deny" | "warn" | "info"
    pub verdict: String,  // "pass" | "warn" | "fail"
    pub ts: i64,
    pub dapp_origin: Option<String>,
    pub method: Option<String>,
    pub decoded_fn: Option<String>,
    pub contract_addr: Option<String>,
    pub contract_symbol: Option<String>,
    pub selector_sig: Option<String>,
    pub selector_decoded: Option<String>,
    pub policy_name: Option<String>,
    pub reason_ko: Option<String>,
    pub reason_en: Option<String>,
}

/// SELECT-result row. All denormalised fields are surfaced as-is so the
/// FE can render historical entries even after a policy is deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerdictRow {
    pub id: i64,
    pub delta_id: Option<i64>,
    pub wallet_id: i64,
    pub policy_id: Option<i64>,
    pub severity: String,
    pub verdict: String,
    pub ts: i64,
    pub dapp_origin: Option<String>,
    pub method: Option<String>,
    pub decoded_fn: Option<String>,
    pub contract_addr: Option<String>,
    pub contract_symbol: Option<String>,
    pub selector_sig: Option<String>,
    pub selector_decoded: Option<String>,
    pub policy_name: Option<String>,
    pub reason_ko: Option<String>,
    pub reason_en: Option<String>,
    pub user_decision: Option<String>,
    pub decided_at: Option<i64>,
}

pub fn insert(tx: &Transaction<'_>, v: &VerdictInsert) -> DbResult<i64> {
    tx.execute(
        "INSERT INTO verdicts (
            delta_id, wallet_id, policy_id,
            severity, verdict, ts,
            dapp_origin, method, decoded_fn,
            contract_addr, contract_symbol,
            selector_sig, selector_decoded,
            policy_name, reason_ko, reason_en
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            v.delta_id,
            v.wallet_id,
            v.policy_id,
            v.severity,
            v.verdict,
            v.ts,
            v.dapp_origin,
            v.method,
            v.decoded_fn,
            v.contract_addr,
            v.contract_symbol,
            v.selector_sig,
            v.selector_decoded,
            v.policy_name,
            v.reason_ko,
            v.reason_en,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

/// Set the user's decision on a `warn` row. Idempotent — re-setting the
/// same value just updates `decided_at`.
pub fn set_decision(tx: &Transaction<'_>, id: i64, decision: &str, now: i64) -> DbResult<bool> {
    let n = tx.execute(
        "UPDATE verdicts SET user_decision = ?2, decided_at = ?3 WHERE id = ?1",
        params![id, decision, now],
    )?;
    Ok(n > 0)
}

pub fn get(tx: &Transaction<'_>, id: i64) -> DbResult<Option<VerdictRow>> {
    let row = tx
        .prepare(SELECT_ALL)?
        .query_row(params![id], row_from_query)
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// Filter spec for `/audit/verdicts` and `/history/verdicts`.
#[derive(Clone, Debug, Default)]
pub struct VerdictFilter {
    /// Lower bound on `ts` (inclusive). `None` = no lower bound.
    pub since_ts: Option<i64>,
    /// Upper bound on `ts` (exclusive). `None` = now.
    pub until_ts: Option<i64>,
    /// Exact verdict match (pass/warn/fail).
    pub verdict: Option<String>,
    /// Exact dapp origin match (hostname).
    pub origin: Option<String>,
    /// Exact policy_id match.
    pub policy_id: Option<i64>,
    /// `wallet_id` match (when set). All wallets when `None`.
    pub wallet_id: Option<i64>,
    /// Free-text substring search on policy_name + reason_en + reason_ko.
    pub search: Option<String>,
    /// Cursor pagination: only rows with `id < before_id`.
    pub before_id: Option<i64>,
    /// Page size; clamped to [1, 500] by the caller.
    pub limit: i64,
}

const SELECT_COLS: &str = "id, delta_id, wallet_id, policy_id, severity, verdict, ts, \
                           dapp_origin, method, decoded_fn, contract_addr, contract_symbol, \
                           selector_sig, selector_decoded, policy_name, reason_ko, reason_en, \
                           user_decision, decided_at";

const SELECT_ALL: &str = "SELECT id, delta_id, wallet_id, policy_id, severity, verdict, ts, \
                          dapp_origin, method, decoded_fn, contract_addr, contract_symbol, \
                          selector_sig, selector_decoded, policy_name, reason_ko, reason_en, \
                          user_decision, decided_at \
                          FROM verdicts WHERE id = ?1";

/// Filtered query. Returns rows ordered by `id DESC` (newest first).
pub fn list_filtered(tx: &Transaction<'_>, f: &VerdictFilter) -> DbResult<Vec<VerdictRow>> {
    let (where_sql, params_vec) = build_where(f);
    let sql = format!(
        "SELECT {SELECT_COLS} FROM verdicts {where_sql} \
         ORDER BY id DESC LIMIT ?{limit_idx}",
        SELECT_COLS = SELECT_COLS,
        limit_idx = params_vec.len() + 1
    );
    let mut bound: Vec<&dyn rusqlite::ToSql> = params_vec
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    bound.push(&f.limit);
    let mut stmt = tx.prepare(&sql)?;
    let rows = stmt
        .query_map(bound.as_slice(), row_from_query)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Aggregate verdict counts for the same filter. Drives `/audit/counts`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerdictCounts {
    pub pass: i64,
    pub warn: i64,
    pub fail: i64,
}

pub fn count_by_verdict(tx: &Transaction<'_>, f: &VerdictFilter) -> DbResult<VerdictCounts> {
    let (where_sql, params_vec) = build_where(f);
    let sql = format!("SELECT verdict, COUNT(*) FROM verdicts {where_sql} GROUP BY verdict");
    let mut stmt = tx.prepare(&sql)?;
    let mut counts = VerdictCounts::default();
    let bound: Vec<&dyn rusqlite::ToSql> = params_vec
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let mut rows = stmt.query(bound.as_slice())?;
    while let Some(r) = rows.next()? {
        let verdict: String = r.get(0)?;
        let n: i64 = r.get(1)?;
        match verdict.as_str() {
            "pass" => counts.pass = n,
            "warn" => counts.warn = n,
            "fail" => counts.fail = n,
            _ => {}
        }
    }
    Ok(counts)
}

// ---------- internals ----------

/// Builds the `WHERE …` clause + bound param list. SQL placeholders use
/// `?N` numbering; the caller appends `LIMIT ?<N+1>` after.
///
/// Returns owned strings rather than borrowing the filter — keeps the
/// `ToSql` lifetimes simple (rusqlite needs `&dyn ToSql` references).
fn build_where(f: &VerdictFilter) -> (String, Vec<String>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    let mut push = |clause: &str, value: String, binds: &mut Vec<String>| {
        let idx = binds.len() + 1;
        let rendered = clause.replace("?N", &format!("?{idx}"));
        clauses.push(rendered);
        binds.push(value);
    };

    if let Some(ts) = f.since_ts {
        push("ts >= ?N", ts.to_string(), &mut binds);
    }
    if let Some(ts) = f.until_ts {
        push("ts < ?N", ts.to_string(), &mut binds);
    }
    if let Some(v) = &f.verdict {
        push("verdict = ?N", v.clone(), &mut binds);
    }
    if let Some(o) = &f.origin {
        push("dapp_origin = ?N", o.clone(), &mut binds);
    }
    if let Some(pid) = f.policy_id {
        push("policy_id = ?N", pid.to_string(), &mut binds);
    }
    if let Some(wid) = f.wallet_id {
        push("wallet_id = ?N", wid.to_string(), &mut binds);
    }
    if let Some(needle) = &f.search {
        // LIKE pattern matches against three columns. Re-using the same
        // bind slot keeps the SQL compact.
        push(
            "(policy_name LIKE ?N OR reason_en LIKE ?N OR reason_ko LIKE ?N)",
            format!("%{needle}%"),
            &mut binds,
        );
    }
    if let Some(before) = f.before_id {
        push("id < ?N", before.to_string(), &mut binds);
    }

    if clauses.is_empty() {
        (String::new(), Vec::new())
    } else {
        (format!("WHERE {}", clauses.join(" AND ")), binds)
    }
}

fn row_from_query(r: &rusqlite::Row<'_>) -> rusqlite::Result<VerdictRow> {
    Ok(VerdictRow {
        id: r.get(0)?,
        delta_id: r.get(1)?,
        wallet_id: r.get(2)?,
        policy_id: r.get(3)?,
        severity: r.get(4)?,
        verdict: r.get(5)?,
        ts: r.get(6)?,
        dapp_origin: r.get(7)?,
        method: r.get(8)?,
        decoded_fn: r.get(9)?,
        contract_addr: r.get(10)?,
        contract_symbol: r.get(11)?,
        selector_sig: r.get(12)?,
        selector_decoded: r.get(13)?,
        policy_name: r.get(14)?,
        reason_ko: r.get(15)?,
        reason_en: r.get(16)?,
        user_decision: r.get(17)?,
        decided_at: r.get(18)?,
    })
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

    fn seed_wallet(tx: &Transaction<'_>, addr: &str) -> i64 {
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

    fn sample_verdict(wid: i64, verdict: &str, origin: &str, ts: i64) -> VerdictInsert {
        VerdictInsert {
            delta_id: None,
            wallet_id: wid,
            policy_id: None,
            severity: if verdict == "fail" {
                "deny".into()
            } else {
                "warn".into()
            },
            verdict: verdict.into(),
            ts,
            dapp_origin: Some(origin.into()),
            method: Some("eth_sendTransaction".into()),
            decoded_fn: Some("swap".into()),
            contract_addr: Some("0x123".into()),
            contract_symbol: Some("Test".into()),
            selector_sig: Some("0xabcdef01".into()),
            selector_decoded: Some("swap(...)".into()),
            policy_name: Some("Max slippage".into()),
            reason_ko: Some("슬리피지 초과".into()),
            reason_en: Some("Slippage exceeded".into()),
        }
    }

    #[test]
    fn insert_get_set_decision_round_trip() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let wid = seed_wallet(tx, "0xa");
            let id = insert(
                tx,
                &sample_verdict(wid, "warn", "app.uniswap.org", 1_730_000_000),
            )?;
            let row = get(tx, id)?.unwrap();
            assert_eq!(row.verdict, "warn");
            assert_eq!(row.dapp_origin.as_deref(), Some("app.uniswap.org"));
            assert!(row.user_decision.is_none());

            assert!(set_decision(tx, id, "trusted", 1_730_000_500)?);
            let row = get(tx, id)?.unwrap();
            assert_eq!(row.user_decision.as_deref(), Some("trusted"));
            assert_eq!(row.decided_at, Some(1_730_000_500));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_filtered_by_verdict_and_origin() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let wid = seed_wallet(tx, "0xa");
            insert(
                tx,
                &sample_verdict(wid, "fail", "uniswap.org", 1_730_000_000),
            )?;
            insert(
                tx,
                &sample_verdict(wid, "warn", "uniswap.org", 1_730_000_100),
            )?;
            insert(
                tx,
                &sample_verdict(wid, "pass", "opensea.io", 1_730_000_200),
            )?;
            insert(
                tx,
                &sample_verdict(wid, "fail", "opensea.io", 1_730_000_300),
            )?;

            // Filter: verdict=fail
            let rows = list_filtered(
                tx,
                &VerdictFilter {
                    verdict: Some("fail".into()),
                    limit: 50,
                    ..Default::default()
                },
            )?;
            assert_eq!(rows.len(), 2);
            // ordered by id DESC → newest first
            assert_eq!(rows[0].ts, 1_730_000_300);

            // Filter: origin=uniswap.org
            let rows = list_filtered(
                tx,
                &VerdictFilter {
                    origin: Some("uniswap.org".into()),
                    limit: 50,
                    ..Default::default()
                },
            )?;
            assert_eq!(rows.len(), 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn count_by_verdict_summary() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let wid = seed_wallet(tx, "0xa");
            insert(tx, &sample_verdict(wid, "pass", "a.com", 1))?;
            insert(tx, &sample_verdict(wid, "pass", "a.com", 2))?;
            insert(tx, &sample_verdict(wid, "warn", "a.com", 3))?;
            insert(tx, &sample_verdict(wid, "fail", "a.com", 4))?;

            let counts = count_by_verdict(
                tx,
                &VerdictFilter {
                    limit: 50,
                    ..Default::default()
                },
            )?;
            assert_eq!(
                counts,
                VerdictCounts {
                    pass: 2,
                    warn: 1,
                    fail: 1
                }
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cursor_pagination_before_id() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let wid = seed_wallet(tx, "0xa");
            let mut ids = Vec::new();
            for ts in 0..10 {
                ids.push(insert(tx, &sample_verdict(wid, "pass", "a.com", ts))?);
            }
            let first_page = list_filtered(
                tx,
                &VerdictFilter {
                    limit: 3,
                    ..Default::default()
                },
            )?;
            assert_eq!(first_page.len(), 3);
            assert_eq!(first_page[0].id, ids[9]);
            assert_eq!(first_page[2].id, ids[7]);

            let cursor = first_page.last().unwrap().id;
            let second_page = list_filtered(
                tx,
                &VerdictFilter {
                    limit: 3,
                    before_id: Some(cursor),
                    ..Default::default()
                },
            )?;
            assert_eq!(second_page.len(), 3);
            assert_eq!(second_page[0].id, ids[6]);
            Ok(())
        })
        .unwrap();
    }
}
