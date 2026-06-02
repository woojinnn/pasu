//! `Perp::ChangeLeverage` lowering → `Perp::ChangeLeverageContext`.

use serde_json::{Map, Value};

use policy_transition::action::perp::ChangeLeverageAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_market_ref, lower_perp_venue};

/// Lower a `ChangeLeverageAction` into the `Perp::ChangeLeverageContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ChangeLeverageAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    m.insert(
        "newLeverage".into(),
        Value::String(action.new_leverage.0.clone()),
    );
    // ChangeLeverageLiveInputs flattened.
    m.insert(
        "maxLeverage".into(),
        Value::String(li.max_leverage.value.0.clone()),
    );
    // `affectedPositions` (Vec<PositionId>) → Set<String>.
    let affected: Vec<Value> = li
        .affected_positions
        .value
        .iter()
        .map(|id| Value::String(id.clone()))
        .collect();
    m.insert("affectedPositions".into(), Value::Array(affected));
    // `newLiqPrices` (Vec<(PositionId, Option<Price>)>) →
    // Set<{ positionId, liqPrice? }>. `liqPrice` is omitted when None.
    let liq: Vec<Value> = li
        .new_liq_prices
        .value
        .iter()
        .map(|(id, price)| {
            let mut e = Map::new();
            e.insert("positionId".into(), Value::String(id.clone()));
            if let Some(price) = price {
                e.insert("liqPrice".into(), Value::String(price.0.clone()));
            }
            Value::Object(e)
        })
        .collect();
    m.insert("newLiqPrices".into(), Value::Array(liq));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"ChangeLeverage""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::{Decimal, Price};
    use policy_transition::action::perp::{
        ChangeLeverageAction, ChangeLeverageLiveInputs, PerpAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_market, sample_venue,
    };

    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
        let action = ChangeLeverageAction {
            venue: sample_venue(),
            market: sample_market(),
            new_leverage: Decimal::new("10"),
            live_inputs: ChangeLeverageLiveInputs {
                max_leverage: live(Decimal::new("20")),
                affected_positions: live(vec!["pos-1".to_owned(), "pos-2".to_owned()]),
                // Exercise both Some and None liqPrice arms.
                new_liq_prices: live(vec![
                    ("pos-1".to_owned(), Some(Price::new("2500"))),
                    ("pos-2".to_owned(), None),
                ]),
            },
        };
        (
            ActionBody::Perp(PerpAction::ChangeLeverage(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn change_leverage_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("change_leverage", &body, &meta);
    }
}
