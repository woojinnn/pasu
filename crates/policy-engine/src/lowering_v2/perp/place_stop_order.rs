//! `Perp::PlaceStopOrder` lowering → `Perp::PlaceStopOrderContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::PlaceStopOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{
    lower_market_ref, lower_perp_account_state, lower_perp_venue, lower_size_spec, perp_side,
    stop_order_kind,
};

/// Lower a `PlaceStopOrderAction` into the `Perp::PlaceStopOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &PlaceStopOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    m.insert("side".into(), Value::String(perp_side(&action.side).into()));
    m.insert("size".into(), lower_size_spec(&action.size));
    m.insert(
        "triggerPrice".into(),
        Value::String(action.trigger_price.0.clone()),
    );
    m.insert(
        "orderKind".into(),
        Value::String(stop_order_kind(&action.order_kind).into()),
    );
    // `limitPrice` is required only for stop_limit / take_profit_limit — OMITTED
    // when absent.
    if let Some(limit_price) = &action.limit_price {
        m.insert("limitPrice".into(), Value::String(limit_price.0.clone()));
    }
    m.insert("reduceOnly".into(), Value::Bool(action.reduce_only));
    // PlaceStopLiveInputs flattened.
    m.insert(
        "markPrice".into(),
        Value::String(li.mark_price.value.0.clone()),
    );
    m.insert(
        "userAccountState".into(),
        lower_perp_account_state(&li.user_account_state.value),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"PlaceStopOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{
        PerpAccountState, PerpAction, PlaceStopLiveInputs, PlaceStopOrderAction, StopOrderKind,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::position::PerpSide;
    use simulation_state::primitives::Price;

    use super::super::test_support::{
        assert_conforms, live, offchain_meta, sample_account_state, sample_account_state_empty,
        sample_market, sample_size, sample_venue,
    };

    /// Build a `PlaceStopOrder` body exercising the requested `side`,
    /// `order_kind`, optional `limit_price`, `reduce_only`, and account-state
    /// branches.
    fn build(
        side: PerpSide,
        order_kind: StopOrderKind,
        limit_price: Option<Price>,
        reduce_only: bool,
        account_state: PerpAccountState,
    ) -> ActionBody {
        let action = PlaceStopOrderAction {
            venue: sample_venue(),
            market: sample_market(),
            side,
            size: sample_size(),
            trigger_price: Price::new("2900"),
            order_kind,
            limit_price,
            reduce_only,
            live_inputs: PlaceStopLiveInputs {
                mark_price: live(Price::new("3050")),
                user_account_state: live(account_state),
            },
        };
        ActionBody::Perp(PerpAction::PlaceStopOrder(action))
    }

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        // StopLimit carries a limitPrice (Some arm) + long side + reduce_only.
        (
            build(
                PerpSide::Long,
                StopOrderKind::StopLimit,
                Some(Price::new("2890")),
                true,
                sample_account_state(),
            ),
            offchain_meta(),
        )
    }

    #[test]
    fn place_stop_order_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("place_stop_order", &body, &meta);
    }

    /// `StopMarket` — a market-triggered stop with **no** `limitPrice` (the
    /// `None` arm; `limitPrice` key is omitted). Also flips to the short side.
    #[test]
    fn place_stop_order_stop_market_no_limit_conforms() {
        let body = build(
            PerpSide::Short,
            StopOrderKind::StopMarket,
            None,
            false,
            sample_account_state(),
        );
        assert_conforms("place_stop_order", &body, &offchain_meta());
    }

    /// `TakeProfit` — market-triggered take-profit with no `limitPrice` (`None`
    /// arm) plus the empty `openPositions` set arm.
    #[test]
    fn place_stop_order_take_profit_no_limit_conforms() {
        let body = build(
            PerpSide::Long,
            StopOrderKind::TakeProfit,
            None,
            false,
            sample_account_state_empty(),
        );
        assert_conforms("place_stop_order", &body, &offchain_meta());
    }

    /// `TakeProfitLimit` — take-profit placed as a limit order, so it carries a
    /// `limitPrice` (the `Some` arm for this `orderKind`).
    #[test]
    fn place_stop_order_take_profit_limit_conforms() {
        let body = build(
            PerpSide::Long,
            StopOrderKind::TakeProfitLimit,
            Some(Price::new("3200")),
            false,
            sample_account_state(),
        );
        assert_conforms("place_stop_order", &body, &offchain_meta());
    }
}
