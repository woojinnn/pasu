//! Hyperliquid L1 account state — the off-chain ledger for one wallet.
//!
//! Wrapped by [`PositionKind::HyperliquidAccount`](super::PositionKind) and held
//! as exactly one [`Position`](super::Position) per wallet. Unlike
//! [`PerpPosition`](super::PerpPosition) (an EVM-ish position with a `U256`
//! `size_base`), every quantity here is a fractional-safe [`Decimal`] because a
//! Hyperliquid `/exchange` payload carries fractional sizes (`"0.1"`) natively.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, Decimal};

/// A wallet's entire Hyperliquid L1 account state.
///
/// `Default` is implemented manually (NOT derived) because [`Decimal`] does not
/// derive `Default` — and a hand-written impl lets `pending_outflow` default to a
/// meaningful `"0"` (rather than an empty string) while `perp_usdc` defaults to
/// `None` (balance not yet synced).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlAccount {
    /// Perp-account USDC margin balance. Moves on withdraw / `usd_send` / fills.
    ///
    /// `None` = balance not synced / unknown (what the reducer always produces
    /// today, since HL account Sync is out of scope). `Some(x)` = a real synced
    /// balance (only a future Sync layer sets this). The withdraw underflow guard
    /// fires only on `Some`; an unsynced (`None`) account has no balance to check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub perp_usdc: Option<Decimal>,
    /// Cumulative USDC outflow intent recorded by withdraw / `usd_send`, kept
    /// even when no base balance is known (so a no-fetch caller still sees it).
    pub pending_outflow: Decimal,
    /// Filled perp positions.
    pub positions: Vec<HlPosition>,
    /// Resting (unfilled) open orders — order intents.
    pub open_orders: Vec<HlOpenOrder>,
    /// Per-asset leverage / margin-mode settings.
    pub leverage_settings: Vec<HlLeverageSetting>,
    /// Delegated agent (API) wallets.
    pub agents: Vec<HlAgentApproval>,
}

impl Default for HlAccount {
    fn default() -> Self {
        Self {
            perp_usdc: None,
            pending_outflow: Decimal::new("0"),
            positions: Vec::new(),
            open_orders: Vec::new(),
            leverage_settings: Vec::new(),
            agents: Vec::new(),
        }
    }
}

/// A filled Hyperliquid perp position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlPosition {
    /// Asset index (`a`): perp = `meta.universe` index; spot = 10000 + spot idx.
    pub asset_index: u32,
    /// Resolved market symbol (e.g. `"BTC"`); `None` until the meta cache resolves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `true` ⇒ long, `false` ⇒ short.
    pub is_long: bool,
    /// Position size in base units.
    pub size: Decimal,
    /// Average entry price.
    pub entry_price: Decimal,
}

/// A resting Hyperliquid open order (an unfilled `hl_order` intent).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlOpenOrder {
    /// Asset index (`a`).
    pub asset_index: u32,
    /// Resolved market symbol; `None` until resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `true` ⇒ buy/long, `false` ⇒ sell/short.
    pub is_buy: bool,
    /// Limit price.
    pub price: Decimal,
    /// Order size in base units.
    pub size: Decimal,
    /// Reduce-only flag.
    pub reduce_only: bool,
    /// Normalized time-in-force tag (`"gtc"` / `"ioc"` / `"post_only"` / ...).
    pub tif: String,
    /// Venue-assigned order id, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub oid: Option<u64>,
    /// Human-readable venue order type, e.g. `"Limit"` or `"Stop Market"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub order_type: Option<String>,
    /// `true` when the order is a trigger order such as TP/SL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub is_trigger: Option<bool>,
    /// Trigger price for TP/SL orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub trigger_price: Option<Decimal>,
    /// Venue-provided trigger condition, e.g. `"Price below 185"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub trigger_condition: Option<String>,
    /// `true` when this is attached to the position TP/SL controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub is_position_tpsl: Option<bool>,
}

/// A per-asset leverage / margin-mode setting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlLeverageSetting {
    /// Asset index (`asset`).
    pub asset_index: u32,
    /// `isCross` — cross (`true`) vs isolated (`false`) margin.
    pub is_cross: bool,
    /// Leverage multiplier.
    pub leverage: u32,
}

/// An agent (API) wallet authorized via `approveAgent`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlAgentApproval {
    /// Agent (API) wallet address being authorized.
    #[tsify(type = "string")]
    pub agent_address: Address,
    /// Optional human-readable agent name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub agent_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl_account_round_trips_through_json() {
        let acct = HlAccount {
            perp_usdc: Some(Decimal::new("1000.5")),
            pending_outflow: Decimal::new("0"),
            positions: vec![HlPosition {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_long: true,
                size: Decimal::new("0.1"),
                entry_price: Decimal::new("60000"),
            }],
            open_orders: vec![HlOpenOrder {
                asset_index: 1,
                symbol: Some("ETH".to_owned()),
                is_buy: false,
                price: Decimal::new("3000"),
                size: Decimal::new("0.25"),
                reduce_only: true,
                tif: "ioc".to_owned(),
                oid: Some(42),
                order_type: Some("Stop Market".to_owned()),
                is_trigger: Some(true),
                trigger_price: Some(Decimal::new("185")),
                trigger_condition: Some("Price below 185".to_owned()),
                is_position_tpsl: Some(true),
            }],
            leverage_settings: vec![HlLeverageSetting {
                asset_index: 0,
                is_cross: true,
                leverage: 5,
            }],
            agents: vec![HlAgentApproval {
                agent_address: Address::from([0x11; 20]),
                agent_name: None,
            }],
        };
        let json = serde_json::to_string(&acct).unwrap();
        // Fractional size preserved (the whole reason HlAccount is Decimal-native).
        assert!(
            json.contains("\"0.1\""),
            "fractional size preserved: {json}"
        );
        let back: HlAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(back, acct);
    }

    #[test]
    fn hl_account_default_is_unsynced_and_empty() {
        let acct = HlAccount::default();
        assert_eq!(acct.perp_usdc, None);
        assert_eq!(acct.pending_outflow, Decimal::new("0"));
        assert!(acct.positions.is_empty());
        assert!(acct.open_orders.is_empty());
        assert!(acct.leverage_settings.is_empty());
        assert!(acct.agents.is_empty());
    }
}
