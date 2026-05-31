//! Sync 에러 타입.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("db error: {0}")]
    Db(#[from] simulation_db::error::DbError),

    #[error("store error: {0}")]
    Store(#[from] simulation_state::store::StoreError),

    #[error("fetch failed: source_id={source_id}, reason={reason}")]
    FetchFailed { source_id: String, reason: String },

    #[error("derive failed: calc_id={calc_id}, reason={reason}")]
    DeriveFailed { calc_id: String, reason: String },

    #[error("cyclic DerivedFrom graph at calc_id={0}")]
    CyclicDeps(String),

    #[error("unknown calc_id: {0}")]
    UnknownCalcId(String),

    #[error("unknown decoder_id: {0}")]
    UnknownDecoder(String),
}

pub type SyncResult<T> = Result<T, SyncError>;
