//! `LiquidStaking::Stake` lowering → `LiquidStaking::StakeContext`.

use serde_json::{Map, Value};

use policy_transition::action::liquid_staking::StakeAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::Stake` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(action: &StakeAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_staking_venue(&action.venue));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(referral) = &action.referral {
        m.insert("referral".into(), Value::String(addr(referral)));
    }
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"LiquidStaking::Action::"Stake""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::liquid_staking::{LiquidStakingAction, StakeAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{lido_venue, onchain_meta, other};

    fn body(referral: bool) -> ActionBody {
        ActionBody::LiquidStaking(LiquidStakingAction::Stake(StakeAction {
            venue: lido_venue(),
            amount: U256::from(1_000_000_000_000_000_000u64),
            referral: if referral { Some(other()) } else { None },
        }))
    }

    #[test]
    fn stake_with_referral_conforms() {
        super::super::test_support::assert_conforms("stake", &body(true), &onchain_meta());
    }

    #[test]
    fn stake_without_referral_conforms() {
        super::super::test_support::assert_conforms("stake", &body(false), &onchain_meta());
    }
}
