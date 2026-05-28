//! `PlaceStopOrderAction` reducer — submit a stop-loss or take-profit
//! trigger order.
//!
//! ## Stop kind dispatch
//!
//! Maps the four `StopOrderKind` variants to `PerpOrderKind`:
//!   - `StopMarket` → `PerpOrderKind::StopMarket`
//!   - `StopLimit` → `PerpOrderKind::StopLimit` (requires `limit_price`)
//!   - `TakeProfit` → `PerpOrderKind::TakeProfit`
//!   - `TakeProfitLimit` → `PerpOrderKind::StopLimit` (limit-flavored TP
//!     groups under the same orderbook lane on most venues; UI / policy
//!     can still distinguish via the source action)
//!
//! ## Trigger price validation
//!
//! Validates the trigger price is on the correct side of the current mark:
//!   - `StopMarket` / `StopLimit` (Long): trigger < mark (stop-loss below entry).
//!     For Short: trigger > mark.
//!   - `TakeProfit` / `TakeProfitLimit` (Long): trigger > mark.
//!     For Short: trigger < mark.
//!
//! Wrong-side triggers fire immediately on submission — venues reject them
//! before they reach the orderbook, so we surface `Invariant` upfront.

use simulation_state::pending::{
    AssetCommitment, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use simulation_state::position::PerpSide;
use simulation_state::{EvalContext, PendingChange, StateDelta, WalletState};

use crate::action::perp::{PlaceStopOrderAction, StopOrderKind};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};

use super::{common, math};

impl Reducer for PlaceStopOrderAction {
    fn apply(&self, _state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        // limit_price is mandatory for *Limit variants.
        match self.order_kind {
            StopOrderKind::StopLimit | StopOrderKind::TakeProfitLimit => {
                if self.limit_price.is_none() {
                    return Err(ReducerError::Invariant(format!(
                        "place_stop: {:?} requires limit_price",
                        self.order_kind
                    )));
                }
            }
            StopOrderKind::StopMarket | StopOrderKind::TakeProfit => {}
        }

        // Trigger side validation against the mark price.
        let trigger = math::parse_decimal(&self.trigger_price)?;
        let mark = math::parse_decimal(&self.live_inputs.mark_price.value)?;
        let is_stop = matches!(
            self.order_kind,
            StopOrderKind::StopMarket | StopOrderKind::StopLimit
        );
        // Stop-Long / TP-Short → trigger must be below mark.
        // Stop-Short / TP-Long → trigger must be above mark.
        let want_below = matches!(
            (is_stop, &self.side),
            (true, PerpSide::Long) | (false, PerpSide::Short)
        );
        let valid_side = if want_below {
            trigger < mark
        } else {
            trigger > mark
        };
        if !valid_side {
            return Err(ReducerError::Invariant(format!(
                "place_stop: trigger {trigger} is on the wrong side of mark {mark} \
                 for {:?} {:?}",
                self.order_kind, self.side
            )));
        }

        let size_base = math::resolve_size_base(&self.size, &self.live_inputs.mark_price.value)?;

        let pending_id = common::pending_id_for_stop_order(
            &self.venue,
            &self.market.symbol,
            &self.side,
            &self.order_kind,
            &self.trigger_price,
        );
        // PerpVenueOrder.price holds the trigger; for *Limit variants we
        // could overload the `price` slot. Phase 2 keeps it simple by
        // storing the trigger; downstream sees the order_kind tag for
        // distinguishing.
        let pending = PendingTx {
            id: pending_id,
            kind: PendingKind::PerpVenueOrder {
                venue: common::venue_ref(&self.venue),
                market: self.market.clone(),
                side: self.side.clone(),
                size_base,
                price: self.trigger_price.clone(),
                order_kind: common::perp_order_kind_from_stop(&self.order_kind),
                reduce_only: self.reduce_only,
            },
            commitment: AssetCommitment::None,
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                valid_until: None,
                nonce: None,
                on_chain_tx: None,
            },
            sync: common::pending_user_source(),
            signed_at: ctx.now,
            signature_payload: Vec::new(),
        };

        delta.pending_changes.push(PendingChange::Add {
            pending: Box::new(pending),
        });
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
    use simulation_state::pending::PerpOrderKind;
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, Time, VenueRef, U256,
    };
    use simulation_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{PerpAccountState, PerpVenue, PlaceStopLiveInputs, SizeSpec};

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn ctx() -> EvalContext {
        use simulation_state::eval_context::RequestKind;
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
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

    fn stop_action(
        kind: StopOrderKind,
        side: PerpSide,
        trigger: &str,
        limit: Option<&str>,
    ) -> PlaceStopOrderAction {
        PlaceStopOrderAction {
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
            trigger_price: Decimal::new(trigger),
            order_kind: kind,
            limit_price: limit.map(Decimal::new),
            reduce_only: true,
            live_inputs: PlaceStopLiveInputs {
                mark_price: live(Decimal::new("3000")),
                user_account_state: live(PerpAccountState {
                    total_collateral_usd: U256::from(10_000_u64),
                    used_margin_usd: U256::ZERO,
                    free_margin_usd: U256::from(10_000_u64),
                    open_positions: vec![],
                }),
            },
        }
    }

    /// `StopMarket` Long: trigger below mark → valid.
    #[test]
    fn stop_market_long_below_mark_emits_pending() {
        let action = stop_action(StopOrderKind::StopMarket, PerpSide::Long, "2900", None);
        let delta = action.apply(&state(), &ctx()).unwrap();
        assert_eq!(delta.pending_changes.len(), 1);
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

    /// `TakeProfit` Long: trigger above mark → valid.
    #[test]
    fn take_profit_long_above_mark_emits_pending() {
        let action = stop_action(StopOrderKind::TakeProfit, PerpSide::Long, "3100", None);
        let delta = action.apply(&state(), &ctx()).unwrap();
        assert_eq!(delta.pending_changes.len(), 1);
    }

    /// Wrong-side trigger rejected.
    #[test]
    fn stop_market_long_above_mark_rejected() {
        let action = stop_action(StopOrderKind::StopMarket, PerpSide::Long, "3100", None);
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("wrong side")));
    }

    /// `StopLimit` without `limit_price` → Invariant.
    #[test]
    fn stop_limit_without_limit_price_rejected() {
        let action = stop_action(StopOrderKind::StopLimit, PerpSide::Long, "2900", None);
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(
            matches!(err, ReducerError::Invariant(msg) if msg.contains("requires limit_price"))
        );
    }

    /// `TakeProfitLimit` short with trigger below mark → valid. Maps to
    /// `PerpOrderKind::StopLimit` (collapsed lane).
    #[test]
    fn take_profit_limit_short_collapses_to_stop_limit() {
        let action = stop_action(
            StopOrderKind::TakeProfitLimit,
            PerpSide::Short,
            "2900",
            Some("2895"),
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
}
