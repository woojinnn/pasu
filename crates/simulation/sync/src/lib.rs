//! `simulation-sync` — `LiveField` 갱신을 담당하는 Sync Orchestrator.
//!
//! state crate 의 `LiveField` 들을 외부 (RPC/Oracle/Venue API) 에서 가져와
//! `value` / `synced_at` / `confidence` 를 in-place 로 갱신한다.
//! `source` 와 `ttl` 은 불변.
//!
//! 동작 흐름:
//! 1. [`walker`] 가 state 를 traverse 하면서 모든 `LiveField` 수집
//! 2. [`batcher`] 가 같은 `DataSource` 끼리 묶음
//! 3. [`fetchers`] 가 batch 호출 → 결과 받음
//! 4. [`topo`] 가 `DerivedFrom` 의 의존 그래프 위상정렬
//! 5. [`calc`] 가 `DerivedFrom` 의 calc 함수로 계산
//! 6. simulation-db 로 결과 write back
//!
//! reducer 와 달리 외부 IO 가 있으므로 native only — wasm 빌드 안 됨.

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
// Phase 1~11 의 본문은 동작 우선 — 후속 패스에서 # Errors / # Panics doc 보강.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]
#![allow(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::similar_names)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_long_first_doc_paragraph)]
#![allow(clippy::format_push_string)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::single_match_else)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::fn_params_excessive_bools)]
#![allow(clippy::needless_continue)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::redundant_else)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::manual_string_new)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::needless_for_each)]
#![allow(clippy::useless_let_if_seq)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::manual_map)]
#![allow(clippy::useless_format)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::match_wild_err_arm)]
#![allow(clippy::ref_option_ref)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::needless_collect)]
#![allow(clippy::filter_map_next)]
#![allow(clippy::manual_filter_map)]
#![allow(clippy::or_then_unwrap)]
#![allow(clippy::unnecessary_unwrap)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::single_char_pattern)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]

pub mod action_scope;
pub mod action_walk;
pub mod args_resolver;
pub mod batcher;
pub mod calc;
pub mod config;
pub mod error;
pub mod fetchers;
pub mod manifest_v2;
pub mod orchestrator;
pub mod primitives_sync;
pub mod resolver;
pub mod scheduler;
pub mod subscription;
pub mod topo;
pub mod walker;

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
pub use error::{SyncError, SyncResult};
pub use fetchers::abi_decoder::{AbiDecoder, AbiTypeRegistry};
pub use fetchers::oracle::{provider_key, PriceFetcher, RestJsonOracleFetcher};
pub use fetchers::rpc::{
    BlockTag, EthCallRequest, ProviderName, RpcConfig, RpcProvider, RpcRouter,
};
pub use manifest_v2::{
    parse_live_inputs, resolve_placeholders, LiveInputSpec, LiveInputsSpec, ResolveContext,
};
pub use orchestrator::{HyperliquidAccountReport, Orchestrator, RefreshReport};
pub use primitives_sync::PrimitivesReport;
pub use resolver::{resolve_field, resolve_inputs, GlobalValues};
pub use scheduler::{Scheduler, SchedulerConfig, TickReport, WalletStore};
pub use subscription::{BlockSubscription, NewBlock, PollingBlockSubscription};
pub use topo::{topological_sort, DepNode};
pub use walker::{walk_stale, ActionSlot, FieldLocation, StaleField, WalkStats};
