//! `Staking::Cooldown` lowering → `Staking::CooldownContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::CooldownAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::Cooldown` action (Aave safety-module `cooldown()`). No
/// arguments and no live inputs — `{ meta, venue }` only.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &CooldownAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    if let Some(account) = &action.account {
        m.insert("account".into(), Value::String(addr(account)));
    }

    Ok(ctx.lowered(r#"Staking::Action::"Cooldown""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_transition::action::staking::{CooldownAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{aave_safety_module_venue, assert_conforms, onchain_meta};

    #[test]
    fn cooldown_conforms() {
        let body = ActionBody::Staking(StakingAction::Cooldown(CooldownAction {
            venue: aave_safety_module_venue(),
            account: None,
        }));
        assert_conforms("cooldown", &body, &onchain_meta());
    }
}
