//! Shared sub-lowerings reused across new-model action lowerings.
//!
//! Mirrors the legacy [`crate::lowering::common`] split: tiny Cedar-encoding
//! primitives ([`cedar`]), token refs/keys ([`token`]), and the action meta /
//! nature / EIP-712 domain ([`meta`]) that every action context embeds.

pub(crate) mod account;
pub(crate) mod amount;
pub(crate) mod cedar;
pub(crate) mod meta;
pub(crate) mod token;
