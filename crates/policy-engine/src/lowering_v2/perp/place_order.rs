//! `Perp::PlaceOrder` lowering → `Perp::PlaceOrderContext`.
//!
//! Merges the former `place_limit_order` + `place_stop_order` leaves. The
//! order kind becomes a flattened, discriminated `orderType` sub-record; the
//! live inputs are emitted only when present (`live_inputs: Some` on the
//! on-chain path, `None` for Hyperliquid pre-sign).

use serde_json::{Map, Value};

use policy_transition::action::perp::{OrderType, PlaceOrderAction};

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{
    lower_market_ref, lower_perp_account_state, lower_perp_venue, lower_size_spec,
    lower_time_in_force, perp_side, stop_order_kind,
};

/// Lower a `PlaceOrderAction` into the `Perp::PlaceOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &PlaceOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    m.insert("side".into(), Value::String(perp_side(&action.side).into()));
    m.insert("size".into(), lower_size_spec(&action.size));
    m.insert("reduceOnly".into(), Value::Bool(action.reduce_only));

    // Flattened, discriminated `orderType` sub-record.
    let mut ot = Map::new();
    match &action.order_type {
        OrderType::Limit {
            price,
            time_in_force,
        } => {
            ot.insert("kind".into(), Value::String("limit".into()));
            ot.insert("price".into(), Value::String(price.0.clone()));
            ot.insert("timeInForce".into(), lower_time_in_force(time_in_force));
        }
        OrderType::Stop {
            trigger_price,
            order_kind,
            limit_price,
        } => {
            ot.insert("kind".into(), Value::String("stop".into()));
            ot.insert(
                "triggerPrice".into(),
                Value::String(trigger_price.0.clone()),
            );
            ot.insert(
                "orderKind".into(),
                Value::String(stop_order_kind(order_kind).into()),
            );
            // `limitPrice` only for stop_limit / take_profit_limit.
            if let Some(limit_price) = limit_price {
                ot.insert("limitPrice".into(), Value::String(limit_price.0.clone()));
            }
        }
        OrderType::Twap {
            duration_minutes,
            randomize,
        } => {
            ot.insert("kind".into(), Value::String("twap".into()));
            ot.insert(
                "durationMinutes".into(),
                Value::from(i64::from(*duration_minutes)),
            );
            ot.insert("randomize".into(), Value::Bool(*randomize));
        }
    }
    m.insert("orderType".into(), Value::Object(ot));

    // Live inputs (flattened) — present on-chain, omitted for HL pre-sign.
    // `best_bid_ask: (Price, Price)` splits into `bestBid` + `bestAsk`.
    if let Some(li) = &action.live_inputs {
        let (best_bid, best_ask) = &li.best_bid_ask.value;
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
    }
    // `leverage` (host-enriched) and `custom` are OMITTED — filled later by
    // enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"PlaceOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use policy_state::position::PerpSide;
    use policy_state::primitives::{Price, Time};
    use policy_transition::action::perp::{
        OrderType, PerpAccountState, PerpAction, PlaceOrderAction, PlaceOrderLiveInputs,
        StopOrderKind, TimeInForce,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, live, offchain_meta, sample_account_state, sample_market, sample_size,
        sample_venue,
    };

    /// On-chain live inputs (the `Some` shape) — exercises the live-field
    /// emission branch.
    fn sample_live() -> PlaceOrderLiveInputs {
        PlaceOrderLiveInputs {
            mark_price: live(Price::new("3050")),
            best_bid_ask: live((Price::new("3049"), Price::new("3051"))),
            open_orders_count: live(3u32),
            user_account_state: live(sample_account_state()),
        }
    }

    fn build(
        side: PerpSide,
        order_type: OrderType,
        reduce_only: bool,
        live_inputs: Option<PlaceOrderLiveInputs>,
    ) -> ActionBody {
        ActionBody::Perp(PerpAction::PlaceOrder(PlaceOrderAction {
            venue: sample_venue(),
            market: sample_market(),
            side,
            size: sample_size(),
            reduce_only,
            order_type,
            live_inputs,
        }))
    }

    /// On-chain limit order (live inputs present, Gtd TimeInForce arm).
    #[test]
    fn place_order_limit_onchain_conforms() {
        let body = build(
            PerpSide::Short,
            OrderType::Limit {
                price: Price::new("3100"),
                time_in_force: TimeInForce::Gtd {
                    until: Time::from_unix(1_738_500_000),
                },
            },
            false,
            Some(sample_live()),
        );
        assert_conforms("place_order", &body, &offchain_meta());
    }

    /// HL-shaped limit order (live inputs absent) — every non-Gtd TimeInForce.
    #[test]
    fn place_order_limit_hl_shape_no_live_inputs_conforms() {
        for tif in [
            TimeInForce::Gtc,
            TimeInForce::Ioc,
            TimeInForce::Fok,
            TimeInForce::PostOnly,
        ] {
            let body = build(
                PerpSide::Long,
                OrderType::Limit {
                    price: Price::new("3100"),
                    time_in_force: tif,
                },
                false,
                None,
            );
            assert_conforms("place_order", &body, &offchain_meta());
        }
    }

    /// HL-shaped stop order — both the `limitPrice` Some arm (stop_limit) and
    /// the None arm (stop_market).
    #[test]
    fn place_order_stop_hl_shape_conforms() {
        let with_limit = build(
            PerpSide::Long,
            OrderType::Stop {
                trigger_price: Price::new("2900"),
                order_kind: StopOrderKind::StopLimit,
                limit_price: Some(Price::new("2890")),
            },
            true,
            None,
        );
        assert_conforms("place_order", &with_limit, &offchain_meta());

        let market_stop = build(
            PerpSide::Short,
            OrderType::Stop {
                trigger_price: Price::new("3100"),
                order_kind: StopOrderKind::StopMarket,
                limit_price: None,
            },
            false,
            None,
        );
        assert_conforms("place_order", &market_stop, &offchain_meta());
    }

    /// HL-shaped twap order (live inputs absent).
    #[test]
    fn place_order_twap_hl_shape_conforms() {
        let body = build(
            PerpSide::Long,
            OrderType::Twap {
                duration_minutes: 30,
                randomize: true,
            },
            false,
            None,
        );
        assert_conforms("place_order", &body, &offchain_meta());
    }

    /// On-chain stop order with empty account state (live present).
    #[test]
    fn place_order_stop_onchain_empty_account_conforms() {
        let li = PlaceOrderLiveInputs {
            mark_price: live(Price::new("3050")),
            best_bid_ask: live((Price::new("3049"), Price::new("3051"))),
            open_orders_count: live(0u32),
            user_account_state: live(PerpAccountState {
                total_collateral_usd: policy_state::primitives::U256::from(10_000_000_000u64),
                used_margin_usd: policy_state::primitives::U256::ZERO,
                free_margin_usd: policy_state::primitives::U256::from(10_000_000_000u64),
                open_positions: vec![],
            }),
        };
        let body = build(
            PerpSide::Long,
            OrderType::Stop {
                trigger_price: Price::new("3100"),
                order_kind: StopOrderKind::TakeProfit,
                limit_price: None,
            },
            false,
            Some(li),
        );
        assert_conforms("place_order", &body, &offchain_meta());
    }
}
