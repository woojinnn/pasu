//! `LiquidStaking::RequestWithdrawal` lowering → `LiquidStaking::RequestWithdrawalContext`.

use serde_json::{Map, Value};

use policy_transition::action::liquid_staking::RequestWithdrawalAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::RequestWithdrawal` action. No live inputs. `amounts`
/// is a `Set<String>` of per-request U256-hex amounts.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &RequestWithdrawalAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_staking_venue(&action.venue));
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert(
        "amounts".into(),
        Value::Array(
            action
                .amounts
                .iter()
                .map(|a| Value::String(u256_hex(*a)))
                .collect(),
        ),
    );
    m.insert("owner".into(), Value::String(addr(&action.owner)));

    Ok(ctx.lowered(
        r#"LiquidStaking::Action::"RequestWithdrawal""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::liquid_staking::{LiquidStakingAction, RequestWithdrawalAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{lido_venue, onchain_meta, other, steth};

    #[test]
    fn request_withdrawal_conforms() {
        let body = ActionBody::LiquidStaking(LiquidStakingAction::RequestWithdrawal(
            RequestWithdrawalAction {
                venue: lido_venue(),
                token: steth(),
                amounts: vec![U256::from(1_000_000_000_000_000_000u64), U256::from(2u64)],
                owner: other(),
            },
        ));
        super::super::test_support::assert_conforms("request_withdrawal", &body, &onchain_meta());
    }
}
