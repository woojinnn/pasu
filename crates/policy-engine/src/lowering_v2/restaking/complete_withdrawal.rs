//! `Restaking::CompleteWithdrawal` lowering → `Restaking::CompleteWithdrawalContext`.

use serde_json::{Map, Value};

use policy_transition::action::restaking::CompleteWithdrawalAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::CompleteWithdrawal` action. No live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &CompleteWithdrawalAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
    m.insert("staker".into(), Value::String(addr(&action.staker)));
    m.insert("withdrawer".into(), Value::String(addr(&action.withdrawer)));
    m.insert(
        "strategies".into(),
        Value::Array(
            action
                .strategies
                .iter()
                .map(|a| Value::String(addr(a)))
                .collect(),
        ),
    );
    if let Some(receive_as_tokens) = action.receive_as_tokens {
        m.insert("receiveAsTokens".into(), Value::Bool(receive_as_tokens));
    }

    Ok(ctx.lowered(
        r#"Restaking::Action::"CompleteWithdrawal""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::restaking::{CompleteWithdrawalAction, RestakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other};

    fn body(receive_as_tokens: Option<bool>) -> ActionBody {
        ActionBody::Restaking(RestakingAction::CompleteWithdrawal(
            CompleteWithdrawalAction {
                venue: eigenlayer_venue(),
                staker: other(),
                withdrawer: other(),
                strategies: vec![other()],
                receive_as_tokens,
            },
        ))
    }

    #[test]
    fn complete_withdrawal_single_conforms() {
        super::super::test_support::assert_conforms(
            "complete_withdrawal",
            &body(Some(true)),
            &onchain_meta(),
        );
    }

    #[test]
    fn complete_withdrawal_absent_flag_conforms() {
        // Absent/unresolved `receive_as_tokens` (e.g. malformed input) — the
        // optional flag is simply not emitted to the Cedar context. (The normal
        // batch `completeQueuedWithdrawals` now decodes it per-element via
        // `parallel_sources`, so `Some(_)` — covered by `..._single_conforms`.)
        super::super::test_support::assert_conforms(
            "complete_withdrawal",
            &body(None),
            &onchain_meta(),
        );
    }
}
