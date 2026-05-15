//! Calldata → `ActionEnvelope` mappers.
//!
//! New pipeline:
//! ```text
//!   TransactionRequest → Decoder (abi-resolver) → DecodedCall
//!                     → Mapper (mappers::protocols::*) → ActionEnvelope
//! ```
//!
//! Each protocol provides one or more [`Mapper`] implementations that consume
//! an [`abi_resolver::DecodedCall`] and emit `ActionEnvelope`s. Mappers are
//! resolved by `MapperMatchKey` (currently just the decoder id) via a
//! [`MapperRegistry`].
//!
//! Module layout:
//! ```text
//!   mapper.rs                       - Mapper trait + MapperRegistry trait
//!   in_memory_mapper_registry.rs    - HashMap-backed MapperRegistry
//!   token_registry.rs               - TokenRegistry trait + EmptyTokenRegistry
//!   protocols/                      - per-protocol mappers
//!     erc20.rs
//!     uniswap_v2.rs
//!     uniswap_v3.rs
//!     weth.rs
//! ```

pub mod in_memory_mapper_registry;
pub mod mapper;
pub mod protocols;
pub mod token_registry;

pub use in_memory_mapper_registry::{InMemoryMapperRegistry, InMemoryMapperRegistryBuilder};
pub use mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey, MapperRegistry};
pub use token_registry::{EmptyTokenRegistry, TokenMetadata, TokenRegistry};
