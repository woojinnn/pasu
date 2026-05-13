//! Universal Router mappers.
//!
//! `execute.rs` is the outer dispatcher. The `commands/` submodule contains
//! per-command mappers; `execute.rs` iterates `commands` bytes and calls the
//! matching command mapper with `inputs[i]`.
//!
//! Address (current mainnet): `0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af`
//! Address (pre-V4):          `0x3fC91A3afd70395Cd496C647d5a6CC9D4B2b7FAD`

pub mod commands;
pub mod execute;
