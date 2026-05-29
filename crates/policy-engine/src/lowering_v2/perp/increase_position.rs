//! `Perp::IncreasePosition` lowering → `Perp::IncreasePositionContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::IncreasePerpAction;

use super::super::common::cedar::u256_hex;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_perp_account_state, lower_perp_venue, lower_size_spec};

/// Lower an `IncreasePerpAction` into the `Perp::IncreasePositionContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &IncreasePerpAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("positionId".into(), Value::String(action.position_id.clone()));
    m.insert("size".into(), lower_size_spec(&action.size));
    // Optional collateral top-up: split the (TokenRef, U256) pair. Both keys are
    // omitted when absent; `addCollateralAmountNano` is host-populated — OMITTED.
    if let Some((token, amount)) = &action.add_collateral {
        m.insert("addCollateralToken".into(), lower_token_ref(token));
        m.insert(
            "addCollateralAmount".into(),
            Value::String(u256_hex(*amount)),
        );
    }
    m.insert("slippageBp".into(), Value::from(i64::from(action.slippage_bp)));
    // OpenPerpLiveInputs flattened (same 10 fields as OpenPosition).
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

    Ok(ctx.lowered(r#"Perp::Action::"IncreasePosition""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::perp::{IncreasePerpAction, OpenPerpLiveInputs, PerpAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Decimal, Price, U256};

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_account_state, sample_size, sample_token,
        sample_venue,
    };

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = IncreasePerpAction {
            venue: sample_venue(),
            position_id: "pos-123".into(),
            size: sample_size(),
            // Exercise the Some arm: addCollateralToken + addCollateralAmount.
            add_collateral: Some((sample_token(), U256::from(500_000_000u64))),
            slippage_bp: 50,
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
            ActionBody::Perp(PerpAction::IncreasePosition(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn increase_position_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("increase_position", &body, &meta);
    }
}
