//! `Perp::OpenPosition` lowering → `Perp::OpenPositionContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::OpenPerpAction;

use super::super::common::cedar::u256_hex;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{
    lower_market_ref, lower_perp_account_state, lower_perp_venue, lower_size_spec, margin_mode,
    perp_side,
};

/// Lower an `OpenPerpAction` into the `Perp::OpenPositionContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &OpenPerpAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;
    let (collateral_token, collateral_amount) = &action.collateral;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    m.insert("side".into(), Value::String(perp_side(&action.side).into()));
    m.insert("size".into(), lower_size_spec(&action.size));
    m.insert("leverage".into(), Value::String(action.leverage.0.clone()));
    m.insert("collateralToken".into(), lower_token_ref(collateral_token));
    m.insert(
        "collateralAmount".into(),
        Value::String(u256_hex(*collateral_amount)),
    );
    // `collateralAmountNano` / `collateralAmountUsd` are host-populated — OMITTED.
    m.insert(
        "marginMode".into(),
        Value::String(margin_mode(&action.margin_mode).into()),
    );
    m.insert("slippageBp".into(), Value::from(i64::from(action.slippage_bp)));
    m.insert("reduceOnly".into(), Value::Bool(action.reduce_only));
    // OpenPerpLiveInputs flattened.
    m.insert(
        "markPrice".into(),
        Value::String(li.mark_price.value.0.clone()),
    );
    m.insert(
        "oraclePrice".into(),
        Value::String(li.oracle_price.value.0.clone()),
    );
    m.insert(
        "fundingRate".into(),
        Value::String(li.funding_rate.value.0.clone()),
    );
    m.insert(
        "availableOi".into(),
        Value::String(u256_hex(li.available_oi.value)),
    );
    m.insert(
        "maxLeverage".into(),
        Value::String(li.max_leverage.value.0.clone()),
    );
    m.insert(
        "initialMarginBp".into(),
        Value::from(i64::from(li.initial_margin_bp.value)),
    );
    m.insert(
        "maintenanceBp".into(),
        Value::from(i64::from(li.maintenance_bp.value)),
    );
    m.insert(
        "feeTakerBp".into(),
        Value::from(i64::from(li.fee_taker_bp.value)),
    );
    m.insert(
        "feeMakerBp".into(),
        Value::from(i64::from(li.fee_maker_bp.value)),
    );
    m.insert(
        "userAccountState".into(),
        lower_perp_account_state(&li.user_account_state.value),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"OpenPosition""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::perp::{OpenPerpAction, OpenPerpLiveInputs, PerpAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::position::{MarginMode, PerpSide};
    use simulation_state::primitives::{Decimal, Price, U256};

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_account_state, sample_market, sample_size,
        sample_token, sample_venue,
    };

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = OpenPerpAction {
            venue: sample_venue(),
            market: sample_market(),
            side: PerpSide::Long,
            size: sample_size(),
            leverage: Decimal::new("5"),
            collateral: (sample_token(), U256::from(1_000_000_000u64)),
            margin_mode: MarginMode::Cross,
            slippage_bp: 50,
            reduce_only: false,
            live_inputs: OpenPerpLiveInputs {
                mark_price: live(Price::new("3050")),
                oracle_price: live(Price::new("3048")),
                funding_rate: live(Decimal::new("0.0001")),
                available_oi: live(U256::from(5_000_000_000_000u64)),
                max_leverage: live(Decimal::new("20")),
                initial_margin_bp: live(500u32),
                maintenance_bp: live(300u32),
                fee_taker_bp: live(5u32),
                fee_maker_bp: live(2u32),
                user_account_state: live(sample_account_state()),
            },
        };
        (
            ActionBody::Perp(PerpAction::OpenPosition(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn open_position_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("open_position", &body, &meta);
    }
}
