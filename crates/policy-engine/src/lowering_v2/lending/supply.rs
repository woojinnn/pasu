//! `Lending::Supply` lowering → `Lending::SupplyContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::SupplyAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_reserve_state, lower_user_lending_state};

/// Lower a `Lending::Supply` action into the `Lending::SupplyContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(action: &SupplyAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` are host-populated → omitted.
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }
    m.insert(
        "reserveState".into(),
        lower_reserve_state(&action.live_inputs.reserve_state.value),
    );
    m.insert(
        "supplyApy".into(),
        Value::String(action.live_inputs.supply_apy.value.to_string()),
    );
    m.insert(
        "aTokenPriceUsd".into(),
        Value::String(action.live_inputs.a_token_price_usd.value.to_string()),
    );
    m.insert(
        "eligibleAsCollat".into(),
        Value::Bool(action.live_inputs.eligible_as_collat.value),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"Supply""#, Value::Object(m)))
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
        LendingAction, SupplyAction, SupplyLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Decimal, U256};

    use super::super::test_support::{
        live, onchain_meta, reserve_state, user_state, usdc, venue,
    };

    /// A representative `Supply` of USDC into Aave V3 (on-behalf-of populated).
    #[test]
    fn supply_lowering_conforms_to_schema() {
        let action = LendingAction::Supply(SupplyAction {
            venue: venue(),
            asset: usdc(),
            amount: U256::from(1_000_000_000u64),
            on_behalf_of: Some(super::super::test_support::user()),
            live_inputs: SupplyLiveInputs {
                reserve_state: live(reserve_state()),
                supply_apy: live(Decimal::new("0.0345")),
                a_token_price_usd: live(Decimal::new("1.00")),
                eligible_as_collat: live(true),
                user_state_before: live(user_state()),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("supply", &body, &meta);
    }
}
