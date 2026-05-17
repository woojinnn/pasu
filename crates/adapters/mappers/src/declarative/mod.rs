//! Declarative DSL (Tier A) for adapter marketplace.
//!
//! Phase 0: serde-able Bundle JSON types only (no execution / mapper impl yet).
//!
//! Spec: `ADAPTER_MARKETPLACE_ARCHITECTURE.md` §4.1, §5.1 (BNF), §5.3 (WhitelistedFn).
//!
//! Module layout (Phase 0):
//! ```text
//!   types.rs   — Bundle JSON struct/enum (serde Deserialize / Serialize)
//! ```
//!
//! Future phases extend with:
//!   - Phase 1: interpreter for `SingleEmit` strategy + DeclarativeMapper
//!   - Phase 4: `MulticallRecurse` execution
//!   - Phase 5: `OpcodeStreamDispatch` execution (Universal Router)
//!   - 향후:    `EnumTaggedDispatch` (Balancer V2 등)

pub mod types;
