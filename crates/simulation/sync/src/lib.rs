//! `simulation-sync` — LiveField 갱신을 담당하는 Sync Orchestrator.
//!
//! state crate 의 LiveField 들을 외부 (RPC/Oracle/Venue API) 에서 가져와
//! `value` / `synced_at` / `confidence` 를 in-place 로 갱신한다.
//! `source` 와 `ttl` 은 불변.
//!
//! 동작 흐름:
//! 1. [`walker`] 가 state 를 traverse 하면서 모든 LiveField 수집
//! 2. [`batcher`] 가 같은 DataSource 끼리 묶음
//! 3. [`fetchers`] 가 batch 호출 → 결과 받음
//! 4. [`topo`] 가 DerivedFrom 의 의존 그래프 위상정렬
//! 5. [`calc`] 가 DerivedFrom 의 calc 함수로 계산
//! 6. simulation-db 로 결과 write back
//!
//! reducer 와 달리 외부 IO 가 있으므로 native only — wasm 빌드 안 됨.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
// Phase 1~11 의 본문은 동작 우선 — 후속 패스에서 # Errors / # Panics doc 보강.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]

pub mod action_scope;
pub mod action_walk;
pub mod batcher;
pub mod calc;
pub mod error;
pub mod fetchers;
pub mod orchestrator;
pub mod primitives_sync;
pub mod resolver;
pub mod scheduler;
pub mod subscription;
pub mod topo;
pub mod walker;

pub use action_scope::{ActionScope, walk_scope};
pub use action_walk::{apply_value_to_action, walk_action_stale};
pub use batcher::{BatchKind, FetchBatch, batch_by_source};
pub use calc::{CalcContext, CalcFn, CalcRegistry};
pub use error::{SyncError, SyncResult};
pub use fetchers::rpc::{
    BlockTag, EthCallRequest, ProviderName, RpcConfig, RpcProvider, RpcRouter,
};
pub use orchestrator::{Orchestrator, RefreshReport};
pub use primitives_sync::PrimitivesReport;
pub use resolver::{GlobalValues, resolve_field, resolve_inputs};
pub use scheduler::{Scheduler, SchedulerConfig, TickReport, WalletStore};
pub use subscription::{BlockSubscription, NewBlock, PollingBlockSubscription};
pub use topo::{DepNode, topological_sort};
pub use walker::{ActionSlot, FieldLocation, StaleField, WalkStats, walk_stale};
