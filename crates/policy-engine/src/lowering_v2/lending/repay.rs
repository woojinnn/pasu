//! `Lending::Repay` lowering → `Lending::RepayContext`.

use serde_json::{Map, Value};

use policy_transition::action::lending::RepayAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_reserve_state, lower_user_lending_state, rate_mode_str};

/// Lower a `Lending::Repay` action into the `Lending::RepayContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(action: &RepayAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` are host-populated → omitted.
    m.insert(
        "rateMode".into(),
        Value::String(rate_mode_str(&action.rate_mode).into()),
    );
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }
    m.insert("useATokens".into(), Value::Bool(action.use_a_tokens));
    m.insert(
        "reserveState".into(),
        lower_reserve_state(&action.live_inputs.reserve_state.value),
    );
    m.insert(
        "currentDebt".into(),
        Value::String(u256_hex(action.live_inputs.current_debt.value)),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"Repay""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::{Address, U256};
    use policy_state::token::RateMode;
    use policy_transition::action::lending::{LendingAction, RepayAction, RepayLiveInputs};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{live, onchain_meta, reserve_state, usdc, user_state, venue};

    /// Build a `Repay` body with a chosen `rate_mode`, `on_behalf_of`, and
    /// `use_a_tokens`, holding the rest fixed.
    fn repay_body(
        rate_mode: RateMode,
        on_behalf_of: Option<Address>,
        use_a_tokens: bool,
    ) -> ActionBody {
        ActionBody::Lending(LendingAction::Repay(RepayAction {
            venue: venue(),
            asset: usdc(),
            amount: U256::MAX,
            rate_mode,
            on_behalf_of,
            use_a_tokens,
            live_inputs: RepayLiveInputs {
                reserve_state: live(reserve_state()),
                current_debt: live(U256::from(250_000_000u64)),
                user_state_before: live(user_state()),
            },
        }))
    }

    /// A representative full-repay (`U256::MAX`) stable-rate using aTokens
    /// (`on_behalf_of == None`).
    #[test]
    fn repay_lowering_conforms_to_schema() {
        let body = repay_body(RateMode::Stable, None, true);
        super::super::test_support::assert_conforms("repay", &body, &onchain_meta());
    }

    /// `rateMode == "variable"`, `onBehalfOf` PRESENT, `useATokens == false` —
    /// exercises the variable spelling + populated-`onBehalfOf` + the other
    /// `useATokens` boolean.
    #[test]
    fn repay_variable_rate_with_on_behalf_of_no_a_tokens_conforms() {
        let body = repay_body(
            RateMode::Variable,
            Some(super::super::test_support::other()),
            false,
        );
        super::super::test_support::assert_conforms("repay", &body, &onchain_meta());
    }

    /// `rateMode == "fixed"` — exercises the third `RateMode` spelling.
    #[test]
    fn repay_fixed_rate_conforms() {
        let body = repay_body(RateMode::Fixed, None, true);
        super::super::test_support::assert_conforms("repay", &body, &onchain_meta());
    }
}
