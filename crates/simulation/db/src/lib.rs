//! `simulation-db` — `simulation-state` 의 타입을 `SQLite` 로 영속화.
//!
//! 사용자당 1 DB 파일 (`~/.scopeball/users/<user_id>/scopeball.db`) 모델.
//! 한 DB 안에 `user_profile` (singleton) + wallets (N개) + 글로벌 token catalog +
//! 지갑별 sparse tables + 변화 로그 (`state_deltas`) 가 같이 들어간다.
//!
//! 세 가지 tx 유형 지원:
//! * **Live** — 익스텐션이 가로챈 tx. status = predicted → pending → confirmed.
//! * **Backfill** — Etherscan API 등으로 가져온 과거 tx. status = historical.
//! * **Simulation** — 시뮬레이션 페이지의 what-if. DB 에 저장하지 않음.
//!
//! 일반적 사용:
//! ```ignore
//! let pool = Pool::open("~/.scopeball/users/google_123/scopeball.db")?;
//! simulation_db::run_migrations(&pool)?;
//! pool.with_tx(|tx| {
//!     repositories::profile::upsert(tx, &profile)?;
//!     repositories::wallets::insert(tx, &wallet)?;
//!     Ok(())
//! })?;
//! ```

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
// Phase 1 본문은 동작 우선 — 후속에서 doc 보강.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::similar_names)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_long_first_doc_paragraph)]
#![allow(clippy::format_push_string)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]

pub mod codec;
pub mod error;
pub mod migrate;
pub mod pool;
pub mod repositories;

pub use error::{DbError, DbResult};
pub use migrate::run as run_migrations;
pub use pool::Pool;
