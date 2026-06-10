//! `policy-sync` refreshes wallet state and action live inputs.
//!
//! The crate has four visible layers:
//! - [`actions`] finds stale action-level live inputs.
//! - [`live`] implements the generic `LiveField<DataSource>` refresh pipeline.
//! - [`sources`] owns external adapters plus bulk primitive/venue snapshots.
//! - [`runtime`] wires the pieces into the orchestrator and polling scheduler.
//!
//! Not every external fact is a `LiveField` today. Balances, approvals, block
//! heights, and Hyperliquid account snapshots are authoritative primitive syncs
//! that replace state in bulk; field-level prices, rates, and action inputs use
//! `LiveField`. Keeping both paths under [`sources`] makes this split explicit.
#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// 의도된 트레이드오프만 allow로 남긴다(사유 필수). 나머지 pedantic은 통과한다.
// 외부 데이터(가격/잔액/레버리지)는 f64 ↔ 정수 변환이 본질이라 cast 계열은 허용.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
// 도메인 어휘가 겹친다(amount/amounts, price/prices 등) — 이름 유사성 경고는 잡음.
#![allow(clippy::similar_names)]
// 어댑터 시그니처는 소유값 전달이 호출부를 단순하게 한다(클론 비용 미미).
#![allow(clippy::needless_pass_by_value)]
// 외부 상태 매핑 match는 같은 본문의 가지가 자연스럽다(의미가 다른 케이스).
#![allow(clippy::match_same_arms)]
// 모듈명이 타입명에 들어가는 관례(sources::SourceError 등)를 따른다.
#![allow(clippy::module_name_repetitions)]
// 동기화 플래그 묶음은 bool 구조체가 정직한 표현이다.
#![allow(clippy::struct_excessive_bools)]
// 긴 오케스트레이션 함수는 선형 흐름이 읽기 좋다 — 분리하면 1회용 헬퍼만 는다.
#![allow(clippy::too_many_lines)]
// 내부 함수의 Errors/Panics 독 섹션은 강제하지 않는다.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod actions;
pub mod live;
pub mod manifests;
pub mod runtime;
pub mod sources;

/// Backwards-compatible path for action scope planning.
pub mod action_scope {
    pub use crate::actions::scope::*;
}

/// Backwards-compatible path for action live-input walking.
pub mod action_walk {
    pub use crate::actions::walk::*;
}

/// Backwards-compatible path for action argument resolution.
pub mod args_resolver {
    pub use crate::actions::args::*;
}

/// Backwards-compatible path for `LiveField` batching.
pub mod batcher {
    pub use crate::live::batcher::*;
}

/// Backwards-compatible path for derived field calculators.
pub mod calc {
    pub use crate::live::calc::*;
}

/// Backwards-compatible path for runtime config.
pub mod config {
    pub use crate::runtime::config::*;
}

/// Backwards-compatible path for sync errors.
pub mod error {
    pub use crate::runtime::error::*;
}

/// Backwards-compatible path for discovery helpers.
pub mod discovery {
    pub use crate::sources::discovery::*;
}

/// Backwards-compatible path for external source adapters.
pub mod fetchers {
    pub use crate::sources::fetchers::*;
}

/// Backwards-compatible path for v2 manifest parsing.
pub mod manifest_v2 {
    pub use crate::manifests::v2::*;
}

/// Backwards-compatible path for the orchestrator.
pub mod orchestrator {
    pub use crate::runtime::orchestrator::*;
}

/// Backwards-compatible path for authoritative primitive sync.
pub mod primitives_sync {
    pub use crate::sources::primitives::*;
}

/// Backwards-compatible path for `LiveField` value resolution.
pub mod resolver {
    pub use crate::live::resolver::*;
}

/// Backwards-compatible path for polling scheduler.
pub mod scheduler {
    pub use crate::runtime::scheduler::*;
}

/// Backwards-compatible path for block subscriptions.
pub mod subscription {
    pub use crate::sources::subscription::*;
}

/// Backwards-compatible path for derived dependency ordering.
pub mod topo {
    pub use crate::live::topo::*;
}

/// Backwards-compatible path for `LiveField` walking.
pub mod walker {
    pub use crate::live::walker::*;
}

pub use action_scope::{walk_scope, ActionScope};
pub use action_walk::{apply_value_to_action, walk_action_stale};
pub use args_resolver::resolve_args;
pub use batcher::{batch_by_source, BatchKind, FetchBatch};
pub use calc::{CalcContext, CalcFn, CalcRegistry};
pub use config::{
    ChainlinkChainConfig, ChainlinkConfig, ChainlinkFeedConfig, HyperliquidConfig, OraclesConfig,
    PythConfig, PythFeedConfig, RestAuthConfig, RestFeedConfig, RestOracleConfig, SyncConfig,
    VenuesConfig,
};
pub use discovery::{
    discover_approvals, discover_top_tokens, fetch_native_balance, CoinGeckoClient,
    DiscoveredApproval, DiscoveredToken, EtherscanClient,
};
pub use error::{SyncError, SyncResult};
pub use fetchers::abi_decoder::{AbiDecoder, AbiTypeRegistry};
pub use fetchers::oracle::{provider_key, PriceFetcher, RestJsonOracleFetcher};
pub use fetchers::rpc::{
    BlockTag, EthCallRequest, ProviderName, RpcConfig, RpcProvider, RpcRouter,
};
pub use manifest_v2::{
    parse_live_inputs, resolve_placeholders, LiveInputSpec, LiveInputsSpec, ResolveContext,
};
pub use orchestrator::{
    HyperliquidAccountReport, IntentOrdersReport, Orchestrator, PermitReconcileReport,
    RefreshReport,
};
pub use primitives_sync::PrimitivesReport;
pub use resolver::{resolve_field, resolve_inputs, GlobalValues};
pub use scheduler::{Scheduler, SchedulerConfig, TickReport, WalletSyncCounts};
// Re-export from policy-state for callers that previously imported the
// trait from `policy-sync` (which is where it used to live).
pub use policy_state::{StoreError, WalletStore};
pub use subscription::{BlockSubscription, NewBlock, PollingBlockSubscription};
pub use topo::{topological_sort, DepNode};
pub use walker::{walk_stale, ActionSlot, FieldLocation, StaleField, WalkStats};
