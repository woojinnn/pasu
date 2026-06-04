//! `Staking::Cooldown` lowering → `Staking::CooldownContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::{CooldownAction, CooldownDenomination};

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::Cooldown` action. Whole-balance (Aave `cooldown()`):
/// `{ meta, venue }`. Partial (Ethena `cooldownShares`/`cooldownAssets`): adds
/// `amount` + `denomination` ("shares" | "assets"). No live inputs.
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
    if let Some(amount) = action.amount {
        m.insert("amount".into(), Value::String(u256_hex(amount)));
    }
    if let Some(denom) = action.denomination {
        let s = match denom {
            CooldownDenomination::Shares => "shares",
            CooldownDenomination::Assets => "assets",
        };
        m.insert("denomination".into(), Value::String(s.into()));
    }

    Ok(ctx.lowered(r#"Staking::Action::"Cooldown""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{
        CooldownAction, CooldownDenomination, StakingAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        aave_safety_module_venue, assert_conforms, ethena_staked_usde_venue, onchain_meta,
    };

    #[test]
    fn cooldown_conforms() {
        let body = ActionBody::Staking(StakingAction::Cooldown(CooldownAction {
            venue: aave_safety_module_venue(),
            account: None,
            amount: None,
            denomination: None,
        }));
        assert_conforms("cooldown", &body, &onchain_meta());
    }

    #[test]
    fn ethena_cooldown_shares_conforms() {
        let body = ActionBody::Staking(StakingAction::Cooldown(CooldownAction {
            venue: ethena_staked_usde_venue(),
            account: None,
            amount: Some(U256::from(1_000_000_000_000_000_000u64)),
            denomination: Some(CooldownDenomination::Shares),
        }));
        assert_conforms("cooldown", &body, &onchain_meta());
    }

    #[test]
    fn ethena_cooldown_assets_conforms() {
        let body = ActionBody::Staking(StakingAction::Cooldown(CooldownAction {
            venue: ethena_staked_usde_venue(),
            account: None,
            amount: Some(U256::from(500_000_000_000_000_000u64)),
            denomination: Some(CooldownDenomination::Assets),
        }));
        assert_conforms("cooldown", &body, &onchain_meta());
    }
}
