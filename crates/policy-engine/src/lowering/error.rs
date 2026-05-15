//! Errors surfaced by lowering an [`ActionEnvelope`] into a `PolicyRequest`.
//!
//! Replacing the previous `Option<PolicyRequest>` return shape with
//! [`Result<PolicyRequest, LoweringError>`] forces callers to make a
//! deliberate decision when an action variant has no per-action lowering
//! implementation yet — silent `None` had been letting future variants bypass
//! the policy engine.
//!
//! [`ActionEnvelope`]: crate::action::ActionEnvelope

use thiserror::Error;

/// Errors produced by [`super::policy_request_from_envelope`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LoweringError {
    /// The action variant has no per-action lowering implementation.
    ///
    /// Callers should treat this as a fails-closed engine error rather than
    /// silently letting the transaction continue.
    #[error("unsupported action variant: {kind}")]
    UnsupportedAction {
        /// Canonical `snake_case` action kind from
        /// [`crate::action::Action::kind`].
        kind: String,
    },
}

#[cfg(test)]
mod tests {
    use super::LoweringError;

    #[test]
    fn unsupported_action_display_includes_kind() {
        let error = LoweringError::UnsupportedAction {
            kind: "claim_rewards".to_owned(),
        };
        assert_eq!(error.to_string(), "unsupported action variant: claim_rewards");
    }
}
