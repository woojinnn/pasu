//! `Lending::Withdraw` lowering → `Lending::WithdrawContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::WithdrawAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_reserve_state, lower_user_lending_state};

/// Lower a `Lending::Withdraw` action into the `Lending::WithdrawContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &WithdrawAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` are host-populated → omitted.
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert(
        "reserveState".into(),
        lower_reserve_state(&action.live_inputs.reserve_state.value),
    );
    m.insert(
        "availableToWithdraw".into(),
        Value::String(u256_hex(action.live_inputs.available_to_withdraw.value)),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"Withdraw""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::lending::{
        LendingAction, WithdrawAction, WithdrawLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

    use super::super::test_support::{
        live, onchain_meta, reserve_state, user, user_state, usdc, venue,
    };

    /// A representative max-withdraw (`U256::MAX`) of USDC from Aave V3.
    #[test]
    fn withdraw_lowering_conforms_to_schema() {
        let action = LendingAction::Withdraw(WithdrawAction {
            venue: venue(),
            asset: usdc(),
            amount: U256::MAX,
            recipient: user(),
            live_inputs: WithdrawLiveInputs {
                reserve_state: live(reserve_state()),
                available_to_withdraw: live(U256::from(500_000_000u64)),
                user_state_before: live(user_state()),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("withdraw", &body, &meta);
    }
}
