//! `PlaceOrderAction` reducer — submit a limit / stop / TWAP order to a perp
//! venue's orderbook. Merges the former `PlaceLimitOrderAction` +
//! `PlaceStopOrderAction` reducers; the order kind is dispatched on
//! [`OrderType`](crate::action::perp::OrderType).
//!
//! ## `live_inputs`
//! `live_inputs` is `Option`. The on-chain `Sync` path always populates it;
//! the Hyperliquid pre-sign path leaves it `None` and is never reduced (it is
//! evaluated through the policy/lowering path, not this stateful reducer). A
//! `None` here is therefore a contract violation → `MissingField`.
//!
//! ## Limit (mirrors the former `place_limit_order` reducer)
//! Emits a `PendingChange::Add` carrying `PendingKind::PerpVenueOrder` priced
//! at the limit, `commitment: AssetCommitment::None` (orderbook venues debit
//! on fill), `lifecycle.valid_until` from `TimeInForce::Gtd`. Validates
//! `open_orders_count` against a soft cap and the limit price against the
//! ±50% mid-spread bracket.
//!
//! ## Stop (mirrors the former `place_stop_order` reducer)
//! Requires `limit_price` for the `*Limit` stop kinds; validates the trigger
//! is on the correct side of mark; maps `StopOrderKind` → `PerpOrderKind`.
//!
//! ## Twap
//! Places resting size priced at mark (no limit price to validate), the same
//! pre-fill effect as a limit. Unreachable from Hyperliquid (which carries no
//! `live_inputs`); present for completeness on any future on-chain TWAP.

use policy_state::pending::{
    AssetCommitment, PendingKind, PendingLifecycle, PendingStatus, PendingTx, PerpOrderKind,
};
use policy_state::position::PerpSide;
use policy_state::primitives::Price;
use policy_state::{EvalContext, PendingChange, StateDelta, WalletState};

use crate::action::perp::{
    OrderType, PlaceOrderAction, PlaceOrderLiveInputs, StopOrderKind, TimeInForce,
};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};

use super::{common, math};

const MAX_OPEN_ORDERS_SOFT_CAP: u32 = 100;

impl Reducer for PlaceOrderAction {
    fn apply(&self, _state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let li = self
            .live_inputs
            .as_ref()
            .ok_or(ReducerError::MissingField("place_order.live_inputs"))?;

        let pending = match &self.order_type {
            OrderType::Limit {
                price,
                time_in_force,
            } => limit_pending(self, li, price, time_in_force, ctx)?,
            OrderType::Stop {
                trigger_price,
                order_kind,
                limit_price,
            } => stop_pending(
                self,
                li,
                trigger_price,
                order_kind,
                limit_price.as_ref(),
                ctx,
            )?,
            OrderType::Twap {
                duration_minutes, ..
            } => twap_pending(self, li, *duration_minutes, ctx)?,
        };

        let mut delta = StateDelta::new();
        delta.pending_changes.push(PendingChange::Add {
            pending: Box::new(pending),
        });
        Ok(delta)
    }
}

/// Build the pending entry for a limit order. Mirrors the former
/// `PlaceLimitOrderAction` reducer.
fn limit_pending(
    action: &PlaceOrderAction,
    li: &PlaceOrderLiveInputs,
    price: &Price,
    time_in_force: &TimeInForce,
    ctx: &EvalContext,
) -> ReducerResult<PendingTx> {
    if li.open_orders_count.value >= MAX_OPEN_ORDERS_SOFT_CAP {
        return Err(ReducerError::Invariant(format!(
            "place_order: open_orders_count {} >= soft cap {MAX_OPEN_ORDERS_SOFT_CAP}",
            li.open_orders_count.value
        )));
    }

    // Sanity-check the limit price against the mid-spread.
    let limit = math::parse_decimal(price)?;
    let (best_bid, best_ask) = &li.best_bid_ask.value;
    let bid = math::parse_decimal(best_bid)?;
    let ask = math::parse_decimal(best_ask)?;
    let mid = (bid + ask) / rust_decimal::Decimal::from(2_u32);
    let bracket_lo = mid * rust_decimal::Decimal::new(5, 1); // 0.5
    let bracket_hi = mid * rust_decimal::Decimal::new(15, 1); // 1.5
    if limit < bracket_lo || limit > bracket_hi {
        return Err(ReducerError::Invariant(format!(
            "place_order: limit price {limit} outside ±50% of mid-spread bracket"
        )));
    }

    let size_base = math::resolve_size_base(&action.size, &li.mark_price.value)?;
    if size_base.is_zero() {
        return Err(ReducerError::Invariant(
            "place_order: resolved size_base is zero".into(),
        ));
    }

    let valid_until = match time_in_force {
        TimeInForce::Gtd { until } => Some(*until),
        TimeInForce::Gtc | TimeInForce::Ioc | TimeInForce::Fok | TimeInForce::PostOnly => None,
    };

    let pending_id = common::pending_id_for_limit_order(
        &action.venue,
        &action.market.symbol,
        &action.side,
        price,
    );
    Ok(PendingTx {
        id: pending_id,
        kind: PendingKind::PerpVenueOrder {
            venue: common::venue_ref(&action.venue),
            market: action.market.clone(),
            side: action.side.clone(),
            size_base,
            price: price.clone(),
            order_kind: PerpOrderKind::Limit,
            reduce_only: action.reduce_only,
        },
        commitment: AssetCommitment::None,
        fill_effect: Box::new(StateDelta::new()),
        lifecycle: PendingLifecycle {
            status: PendingStatus::Active,
            valid_until,
            nonce: None,
            on_chain_tx: None,
            raw_status: None,
        },
        sync: common::pending_user_source(),
        signed_at: ctx.now,
        signature_payload: Vec::new(),
    })
}

/// Build the pending entry for a stop / take-profit order. Mirrors the
/// former `PlaceStopOrderAction` reducer.
fn stop_pending(
    action: &PlaceOrderAction,
    li: &PlaceOrderLiveInputs,
    trigger_price: &Price,
    order_kind: &StopOrderKind,
    limit_price: Option<&Price>,
    ctx: &EvalContext,
) -> ReducerResult<PendingTx> {
    // limit_price is mandatory for *Limit variants.
    match order_kind {
        StopOrderKind::StopLimit | StopOrderKind::TakeProfitLimit => {
            if limit_price.is_none() {
                return Err(ReducerError::Invariant(format!(
                    "place_order: {order_kind:?} requires limit_price"
                )));
            }
        }
        StopOrderKind::StopMarket | StopOrderKind::TakeProfit => {}
    }

    // Trigger side validation against the mark price.
    let trigger = math::parse_decimal(trigger_price)?;
    let mark = math::parse_decimal(&li.mark_price.value)?;
    let is_stop = matches!(
        order_kind,
        StopOrderKind::StopMarket | StopOrderKind::StopLimit
    );
    // Stop-Long / TP-Short → trigger must be below mark.
    // Stop-Short / TP-Long → trigger must be above mark.
    let want_below = matches!(
        (is_stop, &action.side),
        (true, PerpSide::Long) | (false, PerpSide::Short)
    );
    let valid_side = if want_below {
        trigger < mark
    } else {
        trigger > mark
    };
    if !valid_side {
        return Err(ReducerError::Invariant(format!(
            "place_order: trigger {trigger} is on the wrong side of mark {mark} \
             for {order_kind:?} {:?}",
            action.side
        )));
    }

    let size_base = math::resolve_size_base(&action.size, &li.mark_price.value)?;

    let pending_id = common::pending_id_for_stop_order(
        &action.venue,
        &action.market.symbol,
        &action.side,
        order_kind,
        trigger_price,
    );
    Ok(PendingTx {
        id: pending_id,
        kind: PendingKind::PerpVenueOrder {
            venue: common::venue_ref(&action.venue),
            market: action.market.clone(),
            side: action.side.clone(),
            size_base,
            price: trigger_price.clone(),
            order_kind: common::perp_order_kind_from_stop(order_kind),
            reduce_only: action.reduce_only,
        },
        commitment: AssetCommitment::None,
        fill_effect: Box::new(StateDelta::new()),
        lifecycle: PendingLifecycle {
            status: PendingStatus::Active,
            valid_until: None,
            nonce: None,
            on_chain_tx: None,
            raw_status: None,
        },
        sync: common::pending_user_source(),
        signed_at: ctx.now,
        signature_payload: Vec::new(),
    })
}

/// Build the pending entry for a TWAP order — resting size priced at mark.
fn twap_pending(
    action: &PlaceOrderAction,
    li: &PlaceOrderLiveInputs,
    duration_minutes: u32,
    ctx: &EvalContext,
) -> ReducerResult<PendingTx> {
    let size_base = math::resolve_size_base(&action.size, &li.mark_price.value)?;
    if size_base.is_zero() {
        return Err(ReducerError::Invariant(
            "place_order: resolved size_base is zero".into(),
        ));
    }
    let pending_id = common::pending_id_for_twap_order(
        &action.venue,
        &action.market.symbol,
        &action.side,
        duration_minutes,
    );
    Ok(PendingTx {
        id: pending_id,
        kind: PendingKind::PerpVenueOrder {
            venue: common::venue_ref(&action.venue),
            market: action.market.clone(),
            side: action.side.clone(),
            size_base,
            price: li.mark_price.value.clone(),
            order_kind: PerpOrderKind::Limit,
            reduce_only: action.reduce_only,
        },
        commitment: AssetCommitment::None,
        fill_effect: Box::new(StateDelta::new()),
        lifecycle: PendingLifecycle {
            status: PendingStatus::Active,
            valid_until: None,
            nonce: None,
            on_chain_tx: None,
            raw_status: None,
        },
        sync: common::pending_user_source(),
        signed_at: ctx.now,
        signature_payload: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Decimal, MarketRef, Time, VenueRef, U256};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{
        OrderType, PerpAccountState, PerpVenue, PlaceOrderLiveInputs, SizeSpec, StopOrderKind,
        TimeInForce,
    };

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        use policy_state::eval_context::RequestKind;
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn live<T>(value: T) -> LiveField<T> {
        LiveField::new(
            value,
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "ETH/USD".into(),
            },
            now(),
        )
    }

    fn state() -> WalletState {
        WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]))
    }

    fn sample_li() -> PlaceOrderLiveInputs {
        PlaceOrderLiveInputs {
            mark_price: live(Decimal::new("3000")),
            best_bid_ask: live((Decimal::new("2995"), Decimal::new("3005"))),
            open_orders_count: live(5),
            user_account_state: live(PerpAccountState {
                total_collateral_usd: U256::from(10_000_u64),
                used_margin_usd: U256::ZERO,
                free_margin_usd: U256::from(10_000_u64),
                open_positions: vec![],
            }),
        }
    }

    fn base(order_type: OrderType, reduce_only: bool, side: PerpSide) -> PlaceOrderAction {
        PlaceOrderAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side,
            size: SizeSpec::BaseAmount {
                amount: U256::from(1_u64),
            },
            reduce_only,
            order_type,
            live_inputs: Some(sample_li()),
        }
    }

    /// Limit happy path: emit pending Limit order with None commitment.
    #[test]
    fn limit_emits_pending_with_none_commitment() {
        let action = base(
            OrderType::Limit {
                price: Decimal::new("2950"),
                time_in_force: TimeInForce::Gtc,
            },
            false,
            PerpSide::Long,
        );
        let delta = action.apply(&state(), &ctx()).unwrap();
        assert_eq!(delta.pending_changes.len(), 1);
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => {
                assert!(matches!(pending.commitment, AssetCommitment::None));
                match &pending.kind {
                    PendingKind::PerpVenueOrder {
                        order_kind, price, ..
                    } => {
                        assert!(matches!(order_kind, PerpOrderKind::Limit));
                        assert_eq!(price.as_str(), "2950");
                    }
                    _ => panic!("expected PerpVenueOrder"),
                }
                assert!(pending.lifecycle.valid_until.is_none());
            }
            _ => panic!("expected Add"),
        }
    }

    /// GTD time-in-force populates `valid_until`.
    #[test]
    fn limit_gtd_populates_valid_until() {
        let later = Time::from_unix(1_738_086_400);
        let action = base(
            OrderType::Limit {
                price: Decimal::new("2950"),
                time_in_force: TimeInForce::Gtd { until: later },
            },
            false,
            PerpSide::Long,
        );
        let delta = action.apply(&state(), &ctx()).unwrap();
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => {
                assert_eq!(pending.lifecycle.valid_until, Some(later));
            }
            _ => panic!("expected Add"),
        }
    }

    /// Price way outside the ±50% bracket → Invariant.
    #[test]
    fn limit_crazy_price_rejected() {
        let action = base(
            OrderType::Limit {
                price: Decimal::new("100"),
                time_in_force: TimeInForce::Gtc,
            },
            false,
            PerpSide::Long,
        );
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("mid-spread bracket")));
    }

    /// Too many open orders → Invariant.
    #[test]
    fn limit_too_many_orders_rejected() {
        let mut action = base(
            OrderType::Limit {
                price: Decimal::new("2950"),
                time_in_force: TimeInForce::Gtc,
            },
            false,
            PerpSide::Long,
        );
        action.live_inputs.as_mut().unwrap().open_orders_count = live(150);
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("soft cap")));
    }

    /// Missing `live_inputs` (the Hyperliquid pre-sign shape) → `MissingField`.
    #[test]
    fn missing_live_inputs_rejected() {
        let mut action = base(
            OrderType::Limit {
                price: Decimal::new("2950"),
                time_in_force: TimeInForce::Gtc,
            },
            false,
            PerpSide::Long,
        );
        action.live_inputs = None;
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(
            err,
            ReducerError::MissingField("place_order.live_inputs")
        ));
    }

    /// `StopMarket` Long: trigger below mark → valid, maps to `StopMarket`.
    #[test]
    fn stop_market_long_below_mark_emits_pending() {
        let action = base(
            OrderType::Stop {
                trigger_price: Decimal::new("2900"),
                order_kind: StopOrderKind::StopMarket,
                limit_price: None,
            },
            true,
            PerpSide::Long,
        );
        let delta = action.apply(&state(), &ctx()).unwrap();
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => match &pending.kind {
                PendingKind::PerpVenueOrder { order_kind, .. } => {
                    assert!(matches!(order_kind, PerpOrderKind::StopMarket));
                }
                _ => panic!("expected PerpVenueOrder"),
            },
            _ => panic!("expected Add"),
        }
    }

    /// Wrong-side trigger rejected.
    #[test]
    fn stop_market_long_above_mark_rejected() {
        let action = base(
            OrderType::Stop {
                trigger_price: Decimal::new("3100"),
                order_kind: StopOrderKind::StopMarket,
                limit_price: None,
            },
            true,
            PerpSide::Long,
        );
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("wrong side")));
    }

    /// `StopLimit` without `limit_price` → Invariant.
    #[test]
    fn stop_limit_without_limit_price_rejected() {
        let action = base(
            OrderType::Stop {
                trigger_price: Decimal::new("2900"),
                order_kind: StopOrderKind::StopLimit,
                limit_price: None,
            },
            true,
            PerpSide::Long,
        );
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(
            matches!(err, ReducerError::Invariant(msg) if msg.contains("requires limit_price"))
        );
    }

    /// `TakeProfitLimit` short collapses to `StopLimit` lane.
    #[test]
    fn take_profit_limit_short_collapses_to_stop_limit() {
        let action = base(
            OrderType::Stop {
                trigger_price: Decimal::new("2900"),
                order_kind: StopOrderKind::TakeProfitLimit,
                limit_price: Some(Decimal::new("2895")),
            },
            true,
            PerpSide::Short,
        );
        let delta = action.apply(&state(), &ctx()).unwrap();
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => match &pending.kind {
                PendingKind::PerpVenueOrder { order_kind, .. } => {
                    assert!(matches!(order_kind, PerpOrderKind::StopLimit));
                }
                _ => panic!("expected PerpVenueOrder"),
            },
            _ => panic!("expected Add"),
        }
    }

    /// TWAP places resting size priced at mark.
    #[test]
    fn twap_emits_pending_priced_at_mark() {
        let action = base(
            OrderType::Twap {
                duration_minutes: 30,
                randomize: true,
            },
            false,
            PerpSide::Long,
        );
        let delta = action.apply(&state(), &ctx()).unwrap();
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => match &pending.kind {
                PendingKind::PerpVenueOrder {
                    order_kind, price, ..
                } => {
                    assert!(matches!(order_kind, PerpOrderKind::Limit));
                    assert_eq!(price.as_str(), "3000");
                }
                _ => panic!("expected PerpVenueOrder"),
            },
            _ => panic!("expected Add"),
        }
    }
}
