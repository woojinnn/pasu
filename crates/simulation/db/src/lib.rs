//! `simulation-db` — `simulation-state` 의 타입을 SQLite 로 영속화.
//!
//! 글로벌 카탈로그 (`tokens`, `global_live_fields`) + 지갑별 sparse tables
//! (`token_holdings`, `approvals_*`, `positions_*`, `pending_txs`) + 변화 로그
//! (`state_deltas`) 의 스키마 + repository CRUD.
//!
//! Cedar 평가 시의 atomicity 는 SQL transaction (BEGIN/COMMIT/ROLLBACK) 으로 보장:
//! ```ignore
//! db.with_tx(|tx| {
//!     let state = tx.load_wallet_state(&id)?;
//!     let (new_state, delta) = simulation_reducer::apply(&state, &action, &eval)?;
//!     tx.save_wallet_state(&new_state)?;
//!     tx.append_delta(&delta)?;
//!     Ok(())
//! })?;
//! ```

pub mod error;

// 단계적 활성화:
// pub mod pool;          // rusqlite Connection 관리 + Tx wrapper
// pub mod migrate;       // migrations 실행
// pub mod transaction;   // BEGIN/COMMIT/ROLLBACK 헬퍼
// pub mod repositories;  // 테이블별 CRUD
// pub mod codec;         // Rust struct ↔ SQL row 변환
// pub mod queries;       // 자주 쓰는 read query
