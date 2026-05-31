//! `Staking::Unlock` lowering → `Staking::UnlockContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::staking::UnlockAction;

use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::Unlock` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &UnlockAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("token".into(), lower_token_ref(&action.token));

    Ok(ctx.lowered(r#"Staking::Action::"Unlock""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::staking::{StakingAction, UnlockAction};
    use simulation_reducer::action::ActionBody;

    use super::super::test_support::{assert_conforms, crv, onchain_meta, vecrv_venue};

    #[test]
    fn unlock_conforms() {
        let body = ActionBody::Staking(StakingAction::Unlock(UnlockAction {
            venue: vecrv_venue(),
            token: crv(),
        }));
        assert_conforms("unlock", &body, &onchain_meta());
    }
}
