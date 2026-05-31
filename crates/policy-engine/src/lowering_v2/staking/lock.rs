//! `Staking::Lock` lowering → `Staking::LockContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::staking::LockAction;

use super::super::common::cedar::u256_hex;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::Lock` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(action: &LockAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    m.insert(
        "unlockTime".into(),
        Value::String(u256_hex(action.unlock_time)),
    );

    Ok(ctx.lowered(r#"Staking::Action::"Lock""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::staking::{LockAction, StakingAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

    use super::super::test_support::{assert_conforms, crv, onchain_meta, vecrv_venue};

    #[test]
    fn lock_conforms() {
        let body = ActionBody::Staking(StakingAction::Lock(LockAction {
            venue: vecrv_venue(),
            token: crv(),
            amount: U256::from(1_000_000_000_000_000_000u64),
            unlock_time: U256::from(1_900_000_000u64),
        }));
        assert_conforms("lock", &body, &onchain_meta());
    }
}
