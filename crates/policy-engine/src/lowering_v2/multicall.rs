//! Multicall lowering.
//!
//! STUB — Cedar can't express recursive children, so the eventual lowering will
//! emit a flat `Core::MulticallContext` summary (childCount + (domain, action)
//! children) and the SW evaluates each child separately. Until then it returns
//! `Unsupported` (fail-closed).

use simulation_reducer::action::ActionBody;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower an [`ActionBody::Multicall`] (stub).
///
/// Takes the whole [`ActionBody`] (not a domain enum) because `Multicall` is a
/// struct variant on `ActionBody` itself.
///
/// # Errors
///
/// Always returns [`LowerError::Unsupported`] for now.
pub(crate) fn lower(
    _action: &ActionBody,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported("multicall".to_owned()))
}
