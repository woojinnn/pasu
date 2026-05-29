//! `Lending::Liquidate` lowering → `Lending::LiquidateContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::LiquidateAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_user_lending_state};

/// Lower a `Lending::Liquidate` action into the `Lending::LiquidateContext`
/// shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &LiquidateAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("victim".into(), Value::String(addr(&action.victim)));
    m.insert("debtAsset".into(), lower_token_ref(&action.debt_asset));
    m.insert("collatAsset".into(), lower_token_ref(&action.collat_asset));
    m.insert(
        "debtToCover".into(),
        Value::String(u256_hex(action.debt_to_cover)),
    );
    // `debtToCoverNano` / `debtToCoverUsd` are host-populated → omitted.
    m.insert("receiveAToken".into(), Value::Bool(action.receive_a_token));
    m.insert(
        "victimState".into(),
        lower_user_lending_state(&action.live_inputs.victim_state.value),
    );
    m.insert(
        "liquidationBonus".into(),
        Value::from(i64::from(action.live_inputs.liquidation_bonus.value)),
    );
    m.insert(
        "debtAssetPrice".into(),
        Value::String(action.live_inputs.debt_asset_price.value.to_string()),
    );
    m.insert(
        "collatAssetPrice".into(),
        Value::String(action.live_inputs.collat_asset_price.value.to_string()),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"Liquidate""#, Value::Object(m)))
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
        LendingAction, LiquidateAction, LiquidateLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Decimal, U256};

    use super::super::test_support::{live, onchain_meta, other, usdc, user_state, venue, weth};

    /// Build a `Liquidate` body with a chosen `receive_a_token`, holding the
    /// rest fixed.
    fn liquidate_body(receive_a_token: bool) -> ActionBody {
        ActionBody::Lending(LendingAction::Liquidate(LiquidateAction {
            venue: venue(),
            victim: other(),
            debt_asset: usdc(),
            collat_asset: weth(),
            debt_to_cover: U256::from(250_000_000u64),
            receive_a_token,
            live_inputs: LiquidateLiveInputs {
                victim_state: live(user_state()),
                liquidation_bonus: live(500u32),
                debt_asset_price: live(Decimal::new("1.00")),
                collat_asset_price: live(Decimal::new("3050.42")),
            },
        }))
    }

    /// A representative liquidation: cover USDC debt, seize WETH collateral
    /// (`receive_a_token == false`).
    #[test]
    fn liquidate_lowering_conforms_to_schema() {
        let body = liquidate_body(false);
        super::super::test_support::assert_conforms("liquidate", &body, &onchain_meta());
    }

    /// `receiveAToken == true` — exercises the other `receiveAToken` boolean.
    #[test]
    fn liquidate_receive_a_token_conforms() {
        let body = liquidate_body(true);
        super::super::test_support::assert_conforms("liquidate", &body, &onchain_meta());
    }
}
