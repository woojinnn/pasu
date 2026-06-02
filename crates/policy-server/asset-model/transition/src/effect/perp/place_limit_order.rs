//! `PlaceLimitOrderAction` reducer — submit a limit order to a perp venue's
//! orderbook.
//! ## Effect
//! Emits a `PendingChange::Add` carrying a `PendingKind::PerpVenueOrder` with:
//!   - `commitment: AssetCommitment::None` (reduce-only orders lock no margin)
//!     or `HardLock { token, locked: max(notional, 0) }` (the orderbook does
//!     not lock margin until match; we use `None` as the conservative
//!     default and rely on `position_state` to surface the cap).
//!   - `lifecycle.valid_until` populated from `TimeInForce::Gtd { until }`
//!     for time-bound orders, `None` otherwise.
//! ## Per-user limits
//! Validates `open_orders_count` (`LiveField`) against venue maximums via a
//! best-effort 100-order cap — most venues allow at least 100 open orders
//! per market; we surface a soft `Invariant` if the count exceeds that
//! conservative limit. Real per-venue limits surface as a separate
//! `live_inputs.max_open_orders` field in a future schema revision.
//! ## Best bid / ask gating
//! Validates the limit price is "reasonable" — within ±50% of the
//! mid-spread `(best_bid + best_ask) / 2`. Crossed / non-sensical limits
//! land as `Invariant`. The 50% bracket is a sanity check, not a venue
//! constraint; tighter venue-specific validation lives in the policy layer.

use policy_state::pending::{
    AssetCommitment, PendingKind, PendingLifecycle, PendingStatus, PendingTx, PerpOrderKind,
};
use policy_state::{EvalContext, PendingChange, StateDelta, WalletState};

use crate::action::perp::{PlaceLimitOrderAction, TimeInForce};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};

use super::{common, math};

const MAX_OPEN_ORDERS_SOFT_CAP: u32 = 100;

impl Reducer for PlaceLimitOrderAction {
    fn apply(&self, _state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        if self.live_inputs.open_orders_count.value >= MAX_OPEN_ORDERS_SOFT_CAP {
            return Err(ReducerError::Invariant(format!(
                "place_limit: open_orders_count {} >= soft cap {MAX_OPEN_ORDERS_SOFT_CAP}",
                self.live_inputs.open_orders_count.value
            )));
        }

        // Sanity-check the limit price against the mid-spread.
        let limit = math::parse_decimal(&self.price)?;
        let (best_bid, best_ask) = &self.live_inputs.best_bid_ask.value;
        let bid = math::parse_decimal(best_bid)?;
        let ask = math::parse_decimal(best_ask)?;
        let mid = (bid + ask) / rust_decimal::Decimal::from(2_u32);
        let bracket_lo = mid * rust_decimal::Decimal::new(5, 1); // 0.5
        let bracket_hi = mid * rust_decimal::Decimal::new(15, 1); // 1.5
        if limit < bracket_lo || limit > bracket_hi {
            return Err(ReducerError::Invariant(format!(
                "place_limit: limit price {limit} outside ±50% of mid-spread bracket"
            )));
        }

        let size_base = math::resolve_size_base(&self.size, &self.live_inputs.mark_price.value)?;
        if size_base.is_zero() {
            return Err(ReducerError::Invariant(
                "place_limit: resolved size_base is zero".into(),
            ));
        }

        let valid_until = match self.time_in_force {
            TimeInForce::Gtd { until } => Some(until),
            TimeInForce::Gtc | TimeInForce::Ioc | TimeInForce::Fok | TimeInForce::PostOnly => None,
        };

        let pending_id = common::pending_id_for_limit_order(
            &self.venue,
            &self.market.symbol,
            &self.side,
            &self.price,
        );
        let pending = PendingTx {
            id: pending_id,
            kind: PendingKind::PerpVenueOrder {
                venue: common::venue_ref(&self.venue),
                market: self.market.clone(),
                side: self.side.clone(),
                size_base,
                price: self.price.clone(),
                order_kind: PerpOrderKind::Limit,
                reduce_only: self.reduce_only,
            },
            // Limit orders do not lock margin at submission (orderbook
            // venues only debit on fill). Reduce-only orders by definition
            // commit no assets.
            commitment: AssetCommitment::None,
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                valid_until,
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
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::position::PerpSide;
    use policy_state::primitives::{Address, ChainId, Decimal, MarketRef, Time, VenueRef, U256};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{
        PerpAccountState, PerpVenue, PlaceLimitLiveInputs, SizeSpec, TimeInForce,
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

    fn limit_action(price: &str, tif: TimeInForce) -> PlaceLimitOrderAction {
        PlaceLimitOrderAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side: PerpSide::Long,
            size: SizeSpec::BaseAmount {
                amount: U256::from(1_u64),
            },
            price: Decimal::new(price),
            time_in_force: tif,
            reduce_only: false,
            live_inputs: PlaceLimitLiveInputs {
                mark_price: live(Decimal::new("3000")),
                best_bid_ask: live((Decimal::new("2995"), Decimal::new("3005"))),
                open_orders_count: live(5),
                user_account_state: live(PerpAccountState {
                    total_collateral_usd: U256::from(10_000_u64),
                    used_margin_usd: U256::ZERO,
                    free_margin_usd: U256::from(10_000_u64),
                    open_positions: vec![],
                }),
            },
        }
    }

    fn state() -> WalletState {
        WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]))
    }

    /// Happy path: emit pending Limit order with None commitment.
    #[test]
    fn place_limit_emits_pending_with_none_commitment() {
        let action = limit_action("2950", TimeInForce::Gtc);
        let delta = action.apply(&state(), &ctx()).unwrap();
        assert_eq!(delta.pending_changes.len(), 1);
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => {
                assert!(matches!(pending.commitment, AssetCommitment::None));
                match &pending.kind {
                    PendingKind::PerpVenueOrder {
                        order_kind,
                        price,
                        reduce_only,
                        ..
                    } => {
                        assert!(matches!(order_kind, PerpOrderKind::Limit));
                        assert_eq!(price.as_str(), "2950");
                        assert!(!*reduce_only);
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
    fn place_limit_gtd_populates_valid_until() {
        let later = Time::from_unix(1_738_086_400);
        let action = limit_action("2950", TimeInForce::Gtd { until: later });
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
    fn place_limit_crazy_price_rejected() {
        let action = limit_action("100", TimeInForce::Gtc);
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("mid-spread bracket")));
    }

    /// Too many open orders → Invariant.
    #[test]
    fn place_limit_too_many_orders_rejected() {
        let mut action = limit_action("2950", TimeInForce::Gtc);
        action.live_inputs.open_orders_count = live(150);
        let err = action.apply(&state(), &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("soft cap")));
    }
}
