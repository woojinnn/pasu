//! `Staking::IncreaseLockAmount` lowering → `Staking::IncreaseLockAmountContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::IncreaseLockAmountAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::IncreaseLockAmount` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &IncreaseLockAmountAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }

    Ok(ctx.lowered(r#"Staking::Action::"IncreaseLockAmount""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{IncreaseLockAmountAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, crv, onchain_meta, other, vecrv_venue};

    fn body(on_behalf: bool) -> ActionBody {
        ActionBody::Staking(StakingAction::IncreaseLockAmount(
            IncreaseLockAmountAction {
                venue: vecrv_venue(),
                token: crv(),
                amount: U256::from(5_000_000_000_000_000_000u64),
                on_behalf_of: if on_behalf { Some(other()) } else { None },
            },
        ))
    }

    #[test]
    fn increase_amount_self_conforms() {
        assert_conforms("increase_lock_amount", &body(false), &onchain_meta());
    }

    #[test]
    fn increase_amount_deposit_for_conforms() {
        assert_conforms("increase_lock_amount", &body(true), &onchain_meta());
    }
}
