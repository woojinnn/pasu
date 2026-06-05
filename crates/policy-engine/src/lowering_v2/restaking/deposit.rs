//! `Restaking::Deposit` lowering → `Restaking::DepositContext`.

use serde_json::{Map, Value};

use policy_transition::action::restaking::DepositAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_restaking_venue;

/// Lower a `Restaking::Deposit` action. `amount` is in `token` units
/// (user-legible) — no live inputs.
///
/// # Errors
///
/// Infallible today; the `Result` matches the per-action `lower` contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &DepositAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_restaking_venue(&action.venue));
    m.insert("strategy".into(), Value::String(addr(&action.strategy)));
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(staker) = &action.staker {
        m.insert("staker".into(), Value::String(addr(staker)));
    }

    Ok(ctx.lowered(r#"Restaking::Action::"Deposit""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::restaking::{DepositAction, RestakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{eigenlayer_venue, onchain_meta, other, steth};

    fn body(with_staker: bool) -> ActionBody {
        ActionBody::Restaking(RestakingAction::Deposit(DepositAction {
            venue: eigenlayer_venue(),
            strategy: other(),
            token: steth(),
            amount: U256::from(1_000_000_000_000_000_000u64),
            staker: if with_staker { Some(other()) } else { None },
        }))
    }

    #[test]
    fn deposit_direct_conforms() {
        super::super::test_support::assert_conforms("deposit", &body(false), &onchain_meta());
    }

    #[test]
    fn deposit_with_staker_conforms() {
        super::super::test_support::assert_conforms("deposit", &body(true), &onchain_meta());
    }
}
