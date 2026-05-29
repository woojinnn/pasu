//! Token-domain lowering.
//!
//! STUB — the fan-out fills in one leaf module per [`TokenAction`] variant and
//! replaces this dispatch's catch-all. Until then every variant returns
//! `Unsupported` (fail-closed — never a silent pass).

use simulation_reducer::action::token::TokenAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Dispatch a [`TokenAction`] to its per-action lowering.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for every token action (stub).
pub(crate) fn lower(
    action: &TokenAction,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported(format!(
        "token/{}",
        action.action_tag()
    )))
}
