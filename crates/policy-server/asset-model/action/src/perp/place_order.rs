//! `PlaceOrderAction` — unified perp order placement (limit / stop / twap).
//!
//! Replaces the former `PlaceLimitOrderAction` + `PlaceStopOrderAction`. The
//! order kind is a typed [`OrderType`] field — a discriminated enum that
//! enforces per-kind required fields at decode (fail-closed). `live_inputs` is
//! `Option`: the on-chain `Sync` path always populates it, while the
//! data-poor Hyperliquid pre-sign path produces a real perp action with
//! `None` (no fabricated sentinels).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::PerpSide;
use policy_state::primitives::{MarketRef, Price};
use policy_state::LiveField;

use super::{PerpAccountState, PerpVenue, SizeSpec, StopOrderKind, TimeInForce};

/// The order kind of a [`PlaceOrderAction`], with its kind-specific fields.
///
/// Serialized as an internally-tagged record on `kind` (`"limit"` / `"stop"` /
/// `"twap"`); variant fields use serde's default snake_case naming, matching
/// every sibling `PerpAction` body on the JSON / WASM boundary (`time_in_force`,
/// `trigger_price`, `order_kind`, `limit_price`, `duration_minutes`). The typed
/// enum is the fail-closed guarantee — a decoder that omits a per-kind required
/// field fails to deserialize rather than producing a half-formed order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrderType {
    /// A limit order resting on the venue's orderbook.
    Limit {
        /// Limit `Price`.
        price: Price,
        /// Time-in-force policy (`TimeInForce`).
        time_in_force: TimeInForce,
    },
    /// A stop / take-profit trigger order.
    Stop {
        /// Trigger `Price` at which the stop fires.
        trigger_price: Price,
        /// Kind of stop order (`StopOrderKind`).
        order_kind: StopOrderKind,
        /// Required only for `StopLimit` / `TakeProfitLimit`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        limit_price: Option<Price>,
    },
    /// A time-weighted average price order — places resting size split over a
    /// duration. Hyperliquid-originated; carries no limit price.
    Twap {
        /// Duration over which the order is sliced, in minutes.
        duration_minutes: u32,
        /// Whether slice timing is randomized.
        randomize: bool,
    },
}

/// Live inputs read at execution time for a [`PlaceOrderAction`]. The union of
/// the inputs the former limit / stop reducers required (the limit set is the
/// superset). Present on the on-chain path; `None` for Hyperliquid pre-sign.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlaceOrderLiveInputs {
    /// Current mark `Price` for the market.
    pub mark_price: LiveField<Price>,
    /// Best bid / ask `Price` pair used for spread validation.
    pub best_bid_ask: LiveField<(Price, Price)>,
    /// Number of open orders — used to check venue per-user limits.
    pub open_orders_count: LiveField<u32>,
    /// Current `PerpAccountState` for the user.
    pub user_account_state: LiveField<PerpAccountState>,
}

/// Place an order (limit / stop / twap) on a perpetual venue's orderbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlaceOrderAction {
    /// Perpetual venue receiving the order.
    pub venue: PerpVenue,
    /// Market symbol the order is placed on.
    pub market: MarketRef,
    /// Long or short (`PerpSide`).
    pub side: PerpSide,
    /// Order size (`SizeSpec`).
    pub size: SizeSpec,
    /// If `true`, the order may only reduce existing exposure.
    pub reduce_only: bool,
    /// The order kind and its kind-specific fields.
    pub order_type: OrderType,
    /// Live market / account inputs. `None` for Hyperliquid pre-sign orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub live_inputs: Option<PlaceOrderLiveInputs>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::perp::PerpAction;
    use policy_state::position::PerpSide;
    use policy_state::primitives::{ChainId, MarketRef, Price, VenueRef, U256};

    fn sample(order_type: OrderType) -> PlaceOrderAction {
        PlaceOrderAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::arbitrum(),
            },
            market: MarketRef {
                symbol: "BTC-USD".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side: PerpSide::Short,
            size: SizeSpec::BaseAmount {
                amount: U256::from(1u64),
            },
            reduce_only: false,
            order_type,
            live_inputs: None,
        }
    }

    /// The JSON / WASM wire format is snake_case, matching every sibling
    /// `PerpAction` body (`reduce_only`, `order_type`, `time_in_force`). This
    /// pins the convention the Hyperliquid TS decoder (Phase 3) emits against.
    #[test]
    fn place_order_wire_format_is_snake_case() {
        let action = sample(OrderType::Limit {
            price: Price::new("60000"),
            time_in_force: TimeInForce::Gtc,
        });
        let v = serde_json::to_value(&action).unwrap();
        assert!(v.get("reduce_only").is_some(), "snake_case reduce_only");
        assert!(v.get("reduceOnly").is_none(), "no camelCase reduceOnly");
        assert!(
            v.get("live_inputs").is_none(),
            "None live_inputs is omitted"
        );
        let ot = v.get("order_type").expect("snake_case order_type");
        assert_eq!(ot.get("kind").unwrap(), "limit");
        assert!(
            ot.get("time_in_force").is_some(),
            "snake_case time_in_force"
        );
        // Round-trips back to the identical action.
        let back: PlaceOrderAction = serde_json::from_value(v).unwrap();
        assert_eq!(back, action);
    }

    /// The `PerpAction` discriminant is `"action": "place_order"`, and the
    /// stop variant's fields are snake_case (`trigger_price`, `order_kind`).
    #[test]
    fn perp_action_tag_and_stop_fields_snake_case() {
        let pa = PerpAction::PlaceOrder(sample(OrderType::Stop {
            trigger_price: Price::new("59000"),
            order_kind: StopOrderKind::StopMarket,
            limit_price: None,
        }));
        let v = serde_json::to_value(&pa).unwrap();
        assert_eq!(v.get("action").unwrap(), "place_order");
        let ot = v.get("order_type").unwrap();
        assert_eq!(ot.get("kind").unwrap(), "stop");
        assert!(ot.get("trigger_price").is_some());
        assert!(ot.get("order_kind").is_some());
        assert!(ot.get("limit_price").is_none(), "None limit_price omitted");
        let back: PerpAction = serde_json::from_value(v).unwrap();
        assert_eq!(back, pa);
    }
}
