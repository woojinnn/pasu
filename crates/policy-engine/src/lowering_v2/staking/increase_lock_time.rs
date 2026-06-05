//! `Staking::IncreaseLockTime` lowering → `Staking::IncreaseLockTimeContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::IncreaseLockTimeAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::IncreaseLockTime` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &IncreaseLockTimeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert(
        "unlockTime".into(),
        Value::String(u256_hex(action.unlock_time)),
    );

    Ok(ctx.lowered(r#"Staking::Action::"IncreaseLockTime""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{IncreaseLockTimeAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, onchain_meta, vecrv_venue};

    #[test]
    fn increase_unlock_time_conforms() {
        let body = ActionBody::Staking(StakingAction::IncreaseLockTime(IncreaseLockTimeAction {
            venue: vecrv_venue(),
            unlock_time: U256::from(1_950_000_000u64),
        }));
        assert_conforms("increase_lock_time", &body, &onchain_meta());
    }
}
