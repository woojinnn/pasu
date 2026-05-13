//! SQLite-backed function index. Holds a precomputed
//! `(chain_id, address, selector) → FunctionInfo` mapping built from a
//! Sourcify Parquet dump (or any other source feeding the same shape).
//!
//! Schema is fixed:
//!
//! ```sql
//! CREATE TABLE functions (
//!     chain_id  INTEGER NOT NULL,
//!     address   BLOB    NOT NULL,    -- 20 bytes
//!     selector  BLOB    NOT NULL,    --  4 bytes
//!     name      TEXT    NOT NULL,
//!     signature TEXT    NOT NULL,
//!     abi_json  TEXT    NOT NULL,    -- single Function entry, JSON
//!     PRIMARY KEY (chain_id, address, selector)
//! ) WITHOUT ROWID;
//! ```
//!
//! Lookups go through the primary key — O(log N) on the B-tree, in practice
//! a couple of microseconds even for tens of millions of rows.

use crate::sourcify::FunctionInfo;
use alloy_json_abi::Function;
use alloy_primitives::Address;
use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;
use std::sync::Mutex;

/// SQLite-backed Sourcify-style index.
///
/// `Connection` is `Send` but not `Sync`, so we wrap it in `Mutex` to share
/// across threads (axum hands the resolver out to multiple handlers). SQLite
/// reads serialize through the lock; for the dump-DB workload that's fine —
/// queries are sub-millisecond and contention is rare.
pub struct SqliteSourcifyIndex {
    conn: Mutex<Connection>,
}

#[derive(Debug, thiserror::Error)]
pub enum SqliteError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("malformed function ABI JSON in DB: {0}")]
    BadJson(#[from] serde_json::Error),
}

impl SqliteSourcifyIndex {
    /// Open an existing read-only database.
    ///
    /// # Errors
    /// Returns `SqliteError::Sqlite` if the file cannot be opened.
    pub fn open_read_only(path: &Path) -> Result<Self, SqliteError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        // Cache size in pages. 10000 * 4KB = ~40MB cache. Tune as needed.
        conn.pragma_update(None, "cache_size", -10_000)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create or open a writable database, ensuring the `functions` table
    /// exists. Used by the import tool.
    ///
    /// # Errors
    /// Returns `SqliteError::Sqlite` on any DDL failure.
    pub fn open_writable(path: &Path) -> Result<Self, SqliteError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS functions (
                 chain_id  INTEGER NOT NULL,
                 address   BLOB    NOT NULL,
                 selector  BLOB    NOT NULL,
                 name      TEXT    NOT NULL,
                 signature TEXT    NOT NULL,
                 abi_json  TEXT    NOT NULL,
                 PRIMARY KEY (chain_id, address, selector)
             ) WITHOUT ROWID;",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Look up a function by `(chain_id, address, selector)`. Returns `None`
    /// when not present, `Err` when the row is corrupt.
    ///
    /// # Errors
    /// Returns `SqliteError` on any query or JSON parse error.
    pub fn lookup(
        &self,
        chain_id: u64,
        address: &Address,
        selector: [u8; 4],
    ) -> Result<Option<FunctionInfo>, SqliteError> {
        let addr_bytes: [u8; 20] = (*address).into();
        let conn = self
            .conn
            .lock()
            .expect("SqliteSourcifyIndex mutex poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT name, signature, abi_json
             FROM functions
             WHERE chain_id = ?1 AND address = ?2 AND selector = ?3",
        )?;

        let row = stmt
            .query_row(
                params![chain_id as i64, &addr_bytes[..], &selector[..]],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;

        match row {
            None => Ok(None),
            Some((name, signature, abi_json)) => {
                let function: Function = serde_json::from_str(&abi_json)?;
                let arg_names = function
                    .inputs
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>();
                Ok(Some(FunctionInfo {
                    name,
                    signature,
                    arg_names,
                    function,
                }))
            }
        }
    }

    /// Bulk insert API used by the Parquet importer. Pass a stream of
    /// `(chain_id, address, function)` triples; selectors and JSON are
    /// derived from the function. Wraps everything in a single transaction
    /// for speed.
    ///
    /// # Errors
    /// Returns `SqliteError` on any write or JSON error.
    pub fn insert_functions<I>(&self, iter: I) -> Result<usize, SqliteError>
    where
        I: IntoIterator<Item = (u64, Address, Function)>,
    {
        let mut conn = self
            .conn
            .lock()
            .expect("SqliteSourcifyIndex mutex poisoned");
        let tx = conn.transaction()?;
        let mut count = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO functions
                    (chain_id, address, selector, name, signature, abi_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (chain_id, address, function) in iter {
                let selector = function.selector().0;
                let addr_bytes: [u8; 20] = address.into();
                let signature = function.signature();
                let json = serde_json::to_string(&function)?;
                stmt.execute(params![
                    chain_id as i64,
                    &addr_bytes[..],
                    &selector[..],
                    &function.name,
                    &signature,
                    &json,
                ])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    /// Total function count (for diagnostics).
    ///
    /// # Errors
    /// Returns `SqliteError::Sqlite` on query failure.
    pub fn function_count(&self) -> Result<u64, SqliteError> {
        let conn = self
            .conn
            .lock()
            .expect("SqliteSourcifyIndex mutex poisoned");
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM functions", [], |r| r.get(0))?;
        Ok(n as u64)
    }
}

// `optional()` lives in this trait — pull it in at module scope so callers
// don't have to.
use rusqlite::OptionalExtension as _;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn approve_function() -> Function {
        let abi_json = serde_json::json!({
            "name": "approve",
            "type": "function",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount",  "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        });
        serde_json::from_value(abi_json).unwrap()
    }

    #[test]
    fn round_trip_insert_and_lookup() {
        let file = NamedTempFile::new().unwrap();
        let idx = SqliteSourcifyIndex::open_writable(file.path()).unwrap();
        let address = Address::from([0x42u8; 20]);
        let inserted = idx
            .insert_functions(vec![(1, address, approve_function())])
            .unwrap();
        assert_eq!(inserted, 1);
        assert_eq!(idx.function_count().unwrap(), 1);

        let info = idx
            .lookup(1, &address, [0x09, 0x5e, 0xa7, 0xb3])
            .unwrap()
            .expect("approve must be found");
        assert_eq!(info.name, "approve");
        assert_eq!(info.signature, "approve(address,uint256)");
        assert_eq!(info.arg_names, vec!["spender", "amount"]);
    }

    #[test]
    fn lookup_misses() {
        let file = NamedTempFile::new().unwrap();
        let idx = SqliteSourcifyIndex::open_writable(file.path()).unwrap();
        idx.insert_functions(vec![(1, Address::from([0x42u8; 20]), approve_function())])
            .unwrap();
        assert!(idx
            .lookup(1, &Address::from([0xffu8; 20]), [0x09, 0x5e, 0xa7, 0xb3])
            .unwrap()
            .is_none());
        assert!(idx
            .lookup(137, &Address::from([0x42u8; 20]), [0x09, 0x5e, 0xa7, 0xb3])
            .unwrap()
            .is_none());
        assert!(idx
            .lookup(1, &Address::from([0x42u8; 20]), [0xde, 0xad, 0xbe, 0xef])
            .unwrap()
            .is_none());
    }
}
