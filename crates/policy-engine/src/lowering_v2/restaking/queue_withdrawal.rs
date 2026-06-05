//! `Restaking::QueueWithdrawal` lowering → `Restaking::QueueWithdrawalContext`.

use serde_json::{Map, Value};

use policy_transition::action::restaking::QueueWithdrawalAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::QueueWithdrawal` action. `depositShares` is the internal
/// share unit (share→underlying enrichment deferred) — no live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &QueueWithdrawalAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
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
    m.insert(
        "depositShares".into(),
        Value::Array(
            action
                .deposit_shares
                .iter()
                .map(|s| Value::String(u256_hex(*s)))
                .collect(),
        ),
    );
    m.insert("withdrawer".into(), Value::String(addr(&action.withdrawer)));

    Ok(ctx.lowered(r#"Restaking::Action::"QueueWithdrawal""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::restaking::{QueueWithdrawalAction, RestakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other};

    #[test]
    fn queue_withdrawal_conforms() {
        let body = ActionBody::Restaking(RestakingAction::QueueWithdrawal(QueueWithdrawalAction {
            venue: eigenlayer_venue(),
            strategies: vec![other()],
            deposit_shares: vec![U256::from(5_000_000_000_000_000_000u64)],
            withdrawer: other(),
        }));
        super::super::test_support::assert_conforms("queue_withdrawal", &body, &onchain_meta());
    }
}
