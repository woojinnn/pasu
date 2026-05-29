//! `Lending::Borrow` lowering → `Lending::BorrowContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::BorrowAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_reserve_state, lower_user_lending_state, rate_mode_str};

/// Lower a `Lending::Borrow` action into the `Lending::BorrowContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(action: &BorrowAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
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
    m.insert(
        "reserveState".into(),
        lower_reserve_state(&action.live_inputs.reserve_state.value),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    m.insert(
        "assetPriceUsd".into(),
        Value::String(action.live_inputs.asset_price_usd.value.to_string()),
    );
    m.insert(
        "currentBorrowRate".into(),
        Value::String(action.live_inputs.current_borrow_rate.value.to_string()),
    );
    m.insert(
        "availableLiquidity".into(),
        Value::String(u256_hex(action.live_inputs.available_liquidity.value)),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"Borrow""#, Value::Object(m)))
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
        BorrowAction, BorrowLiveInputs, LendingAction,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Decimal, U256};
    use simulation_state::token::RateMode;

    use super::super::test_support::{
        live, onchain_meta, reserve_state, user_state, usdc, venue,
    };

    /// A representative variable-rate `Borrow` of USDC against Aave V3.
    #[test]
    fn borrow_lowering_conforms_to_schema() {
        let action = LendingAction::Borrow(BorrowAction {
            venue: venue(),
            asset: usdc(),
            amount: U256::from(500_000_000u64),
            rate_mode: RateMode::Variable,
            on_behalf_of: None,
            live_inputs: BorrowLiveInputs {
                reserve_state: live(reserve_state()),
                user_state_before: live(user_state()),
                asset_price_usd: live(Decimal::new("1.00")),
                current_borrow_rate: live(Decimal::new("0.0512")),
                available_liquidity: live(U256::from(400_000_000_000u64)),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("borrow", &body, &meta);
    }
}
