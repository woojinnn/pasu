//! Host-injected order-time enrichment for `Perp::PlaceOrder` (Hyperliquid).
//!
//! The HL `/exchange` `order` wire carries only the order *intent* (side, size,
//! price, reduceOnly); the *risk context* a policy wants to gate on — effective
//! leverage, notional USD, account margin health, an existing position's PnL /
//! liquidation proximity — lives in per-(user,asset) venue account state. The
//! service-worker fetches that state from the HL info API (`meta` +
//! `activeAssetData` + `clearinghouseState`, fired concurrently) and injects it
//! here, exactly mirroring how [`AccountLeverage`](super::account::AccountLeverage)
//! injects the `leverage` field and [`TokenDecimals`](super::amount::TokenDecimals)
//! injects `amountNano` siblings.
//!
//! This type carries the fields BEYOND the bare `leverage` (which keeps its own
//! [`AccountLeverage`] map for backward compatibility). Every field is optional:
//! the lowering emits each `Perp::PlaceOrderContext` sibling ONLY when the host
//! resolved it; a miss (info-fetch error / no master / no open position in this
//! market) omits the optional field and a `context has <field>`-guarded policy
//! stays dormant rather than mis-evaluating against a default.
//!
//! Units are pre-scaled to comparable Cedar `Long`s by the host (Cedar cannot do
//! decimal arithmetic): USD amounts are integer dollars, ratios are basis points
//! (bps, 1% = 100), signed where a sign is meaningful (`position_roe_bps`).

use std::collections::BTreeMap;

use serde::Deserialize;

/// Per-market enrichment, keyed by the `Perp::PlaceOrder` body's `market.symbol`
/// (the HL coin, e.g. `"BTC"`) — the same key [`AccountLeverage`] uses.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct MarketEnrichment {
    /// This market's max leverage tier (HL `meta` universe entry).
    #[serde(default)]
    pub max_leverage: Option<i64>,
    /// Margin mode for this market's leverage: `"cross"` | `"isolated"`
    /// (HL `activeAssetData` `leverage.type`).
    #[serde(default)]
    pub leverage_type: Option<String>,
    /// Order notional in USD = `round(size × markPx)` (markPx from
    /// `activeAssetData`, size from the order body).
    #[serde(default)]
    pub notional_usd: Option<i64>,
    /// Signed return-on-equity (bps) of the EXISTING position in this market
    /// (HL `clearinghouseState` `returnOnEquity`); negative = a losing position.
    /// Absent when there is no open position in this market.
    #[serde(default)]
    pub position_roe_bps: Option<i64>,
    /// `|markPx − liquidationPx| / markPx` in bps for this market's position —
    /// how close the existing position is to liquidation. Absent when there is
    /// no open position (or no markPx).
    #[serde(default)]
    pub liquidation_distance_bps: Option<i64>,
    /// Whether an open position already exists in this market
    /// (HL `clearinghouseState` `assetPositions`).
    #[serde(default)]
    pub has_open_position: Option<bool>,
}

/// Account-wide enrichment (HL `clearinghouseState` `marginSummary`) — the same
/// for every market in a batch, so it is not keyed by symbol.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct AccountEnrichment {
    /// Account equity in USD (`marginSummary.accountValue`).
    #[serde(default)]
    pub account_value_usd: Option<i64>,
    /// `totalMarginUsed / accountValue` in bps (margin utilization).
    #[serde(default)]
    pub margin_used_ratio_bps: Option<i64>,
}

/// Host-injected order-time enrichment. An empty value (the [`Default`]) means
/// "nothing resolved" — every accessor yields `None`/empty and the lowering
/// omits every optional enrichment field. Deserialized directly from the
/// `order_enrichment` v2-input field.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct OrderEnrichment {
    /// Per-market enrichment, keyed by `market.symbol`.
    #[serde(default)]
    markets: BTreeMap<String, MarketEnrichment>,
    /// Account-wide enrichment (margin health).
    #[serde(default)]
    account: AccountEnrichment,
}

impl OrderEnrichment {
    /// Build from raw maps (test / non-serde construction).
    #[must_use]
    pub const fn new(
        markets: BTreeMap<String, MarketEnrichment>,
        account: AccountEnrichment,
    ) -> Self {
        Self { markets, account }
    }

    /// Per-market enrichment for a market `symbol`, or `None` when the host did
    /// not inject any (→ the lowering omits every per-market enrichment field).
    #[must_use]
    pub fn market(&self, symbol: &str) -> Option<&MarketEnrichment> {
        self.markets.get(symbol)
    }

    /// Account-wide enrichment (always present as a value; its fields are
    /// individually optional).
    #[must_use]
    pub const fn account(&self) -> &AccountEnrichment {
        &self.account
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_from_wire_and_accessors_read_back() {
        let v = serde_json::json!({
            "markets": {
                "BTC": {
                    "max_leverage": 50,
                    "leverage_type": "cross",
                    "notional_usd": 12000,
                    "position_roe_bps": -1500,
                    "liquidation_distance_bps": 800,
                    "has_open_position": true
                }
            },
            "account": { "account_value_usd": 50000, "margin_used_ratio_bps": 3200 }
        });
        let e: OrderEnrichment = serde_json::from_value(v).unwrap();
        let m = e.market("BTC").unwrap();
        assert_eq!(m.max_leverage, Some(50));
        assert_eq!(m.leverage_type.as_deref(), Some("cross"));
        assert_eq!(m.notional_usd, Some(12000));
        assert_eq!(m.position_roe_bps, Some(-1500));
        assert_eq!(m.liquidation_distance_bps, Some(800));
        assert_eq!(m.has_open_position, Some(true));
        assert_eq!(e.account().account_value_usd, Some(50000));
        assert_eq!(e.account().margin_used_ratio_bps, Some(3200));
        assert!(e.market("ETH").is_none());
    }

    #[test]
    fn empty_and_partial_wire_default_to_none() {
        // Fully empty.
        let e: OrderEnrichment = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(e.market("BTC").is_none());
        assert_eq!(e.account().account_value_usd, None);
        // Partial market entry — unset fields are None, not an error.
        let e: OrderEnrichment = serde_json::from_value(
            serde_json::json!({ "markets": { "BTC": { "notional_usd": 5 } } }),
        )
        .unwrap();
        let m = e.market("BTC").unwrap();
        assert_eq!(m.notional_usd, Some(5));
        assert_eq!(m.max_leverage, None);
        assert_eq!(m.has_open_position, None);
    }

    #[test]
    fn default_is_empty() {
        let e = OrderEnrichment::default();
        assert!(e.market("BTC").is_none());
        assert_eq!(e.account().margin_used_ratio_bps, None);
    }
}
