//! Calldata → `schema_demo` mappers.
//!
//! Each Solidity function or Universal Router command has one mapper file.
//! A mapper:
//!   1. Receives decoded params (or raw calldata + decodes inline),
//!   2. Builds the corresponding `ActionEnvelope`(s) per the JSON schema in
//!      `schema_demo/schema/`,
//!   3. Leaves USD / oracle / token-registry fields as `None` for the host.
//!
//! The `assembler` then wraps the resulting `Vec<ActionEnvelope>` in a
//! `RootRequest` with the top-level metadata.
//!
//! Module layout:
//! ```text
//!   uniswap_v2/        - V2 Router02 (9 swap functions)
//!   uniswap_v3/        - V3 SwapRouter (4 functions)
//!   uniswap_v4/        - V4 Router actions (4 swap actions)
//!   universal_router/  - UR execute() + command dispatcher
//!     commands/        - per-command mappers (V2/V3 sub-dispatch, WRAP, etc.)
//!   types/             - Rust mirrors of schema_demo JSON types
//!   context.rs         - BuildContext, RawTx, TokenRegistry
//!   error.rs           - MapError
//!   assembler.rs       - RootRequest builder
//!   registry.rs        - (chain_id, to, selector) -> mapper dispatch
//! ```

#![allow(dead_code)]
#![allow(clippy::module_inception)]

pub mod assembler;
pub mod context;
pub mod error;
pub mod in_memory_mapper_registry;
pub mod mapper;
pub mod protocols;
pub mod registry;
pub mod token_registry;
pub mod types;

pub mod swap_router_02;
pub mod uniswap_v2;
pub mod uniswap_v3;
pub mod uniswap_v4;
pub mod universal_router;

pub use in_memory_mapper_registry::{InMemoryMapperRegistry, InMemoryMapperRegistryBuilder};
pub use mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey, MapperRegistry};
pub use token_registry::{EmptyTokenRegistry, TokenMetadata, TokenRegistry};
