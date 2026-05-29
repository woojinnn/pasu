//! `Perp::PlaceLimitOrder` lowering → `Perp::PlaceLimitOrderContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::PlaceLimitOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{
    lower_market_ref, lower_perp_account_state, lower_perp_venue, lower_size_spec,
    lower_time_in_force, perp_side,
};

/// Lower a `PlaceLimitOrderAction` into the `Perp::PlaceLimitOrderContext`
/// shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &PlaceLimitOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;
    let (best_bid, best_ask) = &li.best_bid_ask.value;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    m.insert("side".into(), Value::String(perp_side(&action.side).into()));
    m.insert("size".into(), lower_size_spec(&action.size));
    m.insert("price".into(), Value::String(action.price.0.clone()));
    m.insert(
        "timeInForce".into(),
        lower_time_in_force(&action.time_in_force),
    );
    m.insert("reduceOnly".into(), Value::Bool(action.reduce_only));
    // PlaceLimitLiveInputs flattened. `best_bid_ask: (Price, Price)` splits into
    // `bestBid` + `bestAsk`.
    m.insert(
        "markPrice".into(),
        Value::String(li.mark_price.value.0.clone()),
    );
    m.insert("bestBid".into(), Value::String(best_bid.0.clone()));
    m.insert("bestAsk".into(), Value::String(best_ask.0.clone()));
    m.insert(
        "openOrdersCount".into(),
        Value::from(i64::from(li.open_orders_count.value)),
    );
    m.insert(
        "userAccountState".into(),
        lower_perp_account_state(&li.user_account_state.value),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"PlaceLimitOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{
        PerpAction, PlaceLimitLiveInputs, PlaceLimitOrderAction, TimeInForce,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::position::PerpSide;
    use simulation_state::primitives::{Price, Time};

    use super::super::test_support::{
        assert_conforms, live, offchain_meta, sample_account_state, sample_market, sample_size,
        sample_venue,
    };

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = PlaceLimitOrderAction {
            venue: sample_venue(),
            market: sample_market(),
            side: PerpSide::Short,
            size: sample_size(),
            price: Price::new("3100"),
            // Exercise the Gtd arm (carries `until`).
            time_in_force: TimeInForce::Gtd {
                until: Time::from_unix(1_738_500_000),
            },
            reduce_only: false,
            live_inputs: PlaceLimitLiveInputs {
                mark_price: live(Price::new("3050")),
                best_bid_ask: live((Price::new("3049"), Price::new("3051"))),
                open_orders_count: live(3u32),
                user_account_state: live(sample_account_state()),
            },
        };
        (
            ActionBody::Perp(PerpAction::PlaceLimitOrder(action)),
            offchain_meta(),
        )
    }

    #[test]
    fn place_limit_order_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("place_limit_order", &body, &meta);
    }
}
