//! Unknown-action lowering.
//!
//! STUB — the eventual lowering emits a `Core::UnknownContext`
//! (target/chain/calldata/value) so policies can still gate raw calls. Until
//! then it returns `Unsupported` (fail-closed).

use simulation_reducer::action::ActionBody;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower an [`ActionBody::Unknown`] (stub).
///
/// Takes the whole [`ActionBody`] (not a domain enum) because `Unknown` is a
/// struct variant on `ActionBody` itself.
///
/// # Errors
///
/// Always returns [`LowerError::Unsupported`] for now.
pub(crate) fn lower(
    _action: &ActionBody,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported("unknown".to_owned()))
}
