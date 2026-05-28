//! Declarative DSL (Tier A) for adapter loader.
//!
//! Phase 0: serde-able Bundle JSON types only (no execution / mapper impl yet).
//!
//! Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §4.1, §5.1 (BNF), §5.3 (WhitelistedFn).
//!
//! Module layout (Phase 1A):
//! ```text
//!   types.rs        — Bundle JSON struct/enum (serde Deserialize / Serialize)
//!   builtin_fn.rs   — whitelisted functions (Phase 1A: select_address)
//!   eval.rs         — ValueExpr evaluator + JsonPath walker
//!   single_emit.rs  — single_emit strategy → ActionEnvelope
//!   mapper.rs       — DeclarativeMapper (impl Mapper)
//! ```
//!
//! Future phases extend with:
//!   - Phase 4: `MulticallRecurse` execution
//!   - Phase 5: `OpcodeStreamDispatch` execution (Universal Router)
//!   - 향후:    `EnumTaggedDispatch` (Balancer V2 등)

pub mod action_builder;
pub mod array_emit;
pub mod builtin_fn;
pub mod enum_tagged;
pub mod eval;
pub mod mapper;
pub mod multicall;
pub mod opcode_stream;
pub mod single_emit;
pub mod types;

pub use mapper::DeclarativeMapper;
pub use types::{AdapterFunctionBundle, EmitRule, ValueExpr};
