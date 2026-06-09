//! Host-injected venue account state shared across new-model lowerings.
//!
//! Some venue actions carry no leverage/margin in their signed wire form —
//! Hyperliquid's `/exchange` `order` leg, for instance, has no leverage field;
//! the effective leverage is per-(user,asset) account state the venue applies
//! at fill (set separately via `updateLeverage`). To let a policy gate an ORDER
//! on its effective leverage (not just on the leverage-change action), the host
//! service-worker fetches that state from the venue's info API and injects it
//! here — exactly mirroring how [`TokenDecimals`](super::amount::TokenDecimals)
//! injects registry-fetched decimals for the `amountNano` siblings.
//!
//! The lowering fills the optional `leverage` context field ONLY when the host
//! resolved it; when it is unknown (info-fetch miss / unknown account /
//! timeout) the field is omitted (the cedarschema sibling is optional), so a
//! `context has leverage`-guarded policy simply stays dormant rather than
//! mis-evaluating against a default.

use std::collections::BTreeMap;

/// Host-injected effective per-asset leverage, keyed by the market **symbol**
/// (HL coin, e.g. `"BTC"`). Built by the service-worker from on-demand
/// `activeAssetData` info lookups; an empty map (the [`Default`]) means "no
/// leverage known" — every [`Self::leverage_for_symbol`] returns `None` and the
/// lowering omits the optional `leverage` field.
///
/// Keyed by symbol (not the venue `asset_index`) because the generic
/// `Perp::PlaceOrder` body carries `market.symbol`, not the HL numeric index.
/// String keys match the JSON object the host sends and mirror
/// [`TokenDecimals`](super::amount::TokenDecimals)' address-keyed map.
#[derive(Debug, Default, Clone)]
pub struct AccountLeverage(BTreeMap<String, i64>);

impl AccountLeverage {
    /// Build from a raw `market_symbol → leverage` map.
    #[must_use]
    pub const fn new(map: BTreeMap<String, i64>) -> Self {
        Self(map)
    }

    /// Effective leverage for a market `symbol`, or `None` when the host did not
    /// inject it (→ the lowering omits the optional `leverage` field).
    #[must_use]
    pub fn leverage_for_symbol(&self, symbol: &str) -> Option<i64> {
        self.0.get(symbol).copied()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn leverage_for_symbol_returns_injected_value() {
        let mut map = BTreeMap::new();
        map.insert("BTC".to_owned(), 26i64);
        map.insert("ETH".to_owned(), 3i64);
        let lev = AccountLeverage::new(map);
        assert_eq!(lev.leverage_for_symbol("BTC"), Some(26));
        assert_eq!(lev.leverage_for_symbol("ETH"), Some(3));
    }

    #[test]
    fn leverage_for_absent_symbol_is_none() {
        let lev = AccountLeverage::new(BTreeMap::new());
        assert_eq!(lev.leverage_for_symbol("BTC"), None);
        // Default (empty) map → always None.
        assert_eq!(AccountLeverage::default().leverage_for_symbol("SOL"), None);
    }
}
