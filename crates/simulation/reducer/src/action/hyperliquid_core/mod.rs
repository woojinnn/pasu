//! Hyperliquid CORE actions — the thin, off-chain L1 action model.
//!
//! Unlike [`PerpAction`](crate::action::perp::PerpAction), which carries
//! venue-live inputs (mark price, order book, account state) that an order
//! payload does NOT contain, a Hyperliquid `/exchange` request is a small,
//! self-describing JSON intent signed by an agent key. This module models only
//! the order-/transfer-intrinsic fields the request actually carries, so the
//! policy engine can evaluate it WITHOUT fetching any live data from the venue.
//!
//! v1 covers the high-risk subset: an order, a leverage change, and the three
//! fund-movement / delegation actions (`withdraw3`, `usdSend`, `approveAgent`)
//! that move or authorize control of funds.
//!
//! ## Tag naming
//!
//! The serde `action` tags are prefixed `hl_` (`hl_order`, `hl_withdraw`, …)
//! so they are globally unique across every domain's action set — notably
//! `withdraw` is already a Lending tag, and the engine's flat action registries
//! require unique tags. Policies match on these prefixed tags.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, Decimal};

/// A Hyperliquid CORE action, decoded from a `/exchange` POST body.
///
/// The serde `action` tag is the source of truth for the trigger tag a policy
/// matches on; [`Self::action_tag`] returns the same string and is verified
/// against serde by the `action_tag_matches_serde` test.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action")]
pub enum HyperliquidCoreAction {
    /// Place an order (`{"type":"order"}`, one leg of `orders[]`).
    #[serde(rename = "hl_order")]
    Order(HlOrderAction),
    /// Change leverage for a market (`{"type":"updateLeverage"}`).
    #[serde(rename = "hl_update_leverage")]
    UpdateLeverage(HlUpdateLeverageAction),
    /// Withdraw USDC off the L1 to a destination (`{"type":"withdraw3"}`).
    #[serde(rename = "hl_withdraw")]
    Withdraw(HlWithdrawAction),
    /// Send USDC to another account (`{"type":"usdSend"}`).
    #[serde(rename = "hl_usd_send")]
    UsdSend(HlUsdSendAction),
    /// Authorize an agent (API) wallet to sign on the account's behalf
    /// (`{"type":"approveAgent"}`).
    #[serde(rename = "hl_approve_agent")]
    ApproveAgent(HlApproveAgentAction),
}

impl HyperliquidCoreAction {
    /// The serde `action` tag — the trigger tag a policy matches on.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Order(_) => "hl_order",
            Self::UpdateLeverage(_) => "hl_update_leverage",
            Self::Withdraw(_) => "hl_withdraw",
            Self::UsdSend(_) => "hl_usd_send",
            Self::ApproveAgent(_) => "hl_approve_agent",
        }
    }

    /// Every Hyperliquid CORE action is on the `"hyperliquid"` venue, so policies
    /// can scope on `context.venue.name == "hyperliquid"`.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        Some("hyperliquid")
    }
}

/// Place-order leg: `orders[i]` of a `{"type":"order"}` action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlOrderAction {
    /// Asset index (`a`): perp = `meta.universe` index; spot = 10000 + spot idx.
    pub asset_index: u32,
    /// Resolved market symbol (e.g. `"BTC"`); `None` until the venue meta cache
    /// resolves the numeric index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `b` — `true` ⇒ long/buy, `false` ⇒ short/sell.
    pub is_buy: bool,
    /// Limit price (`p`), a decimal value held as a string (fractional-safe).
    pub price: Decimal,
    /// Size in base units (`s`), a decimal value held as a string.
    pub size: Decimal,
    /// `r` — reduce-only.
    pub reduce_only: bool,
    /// Time-in-force (`gtc` / `ioc` / `post_only`), normalized from `t`.
    pub tif: String,
}

/// Leverage change: `{"type":"updateLeverage"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUpdateLeverageAction {
    /// Asset index (`asset`).
    pub asset_index: u32,
    /// Resolved market symbol; `None` until resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `isCross` — cross (`true`) vs isolated (`false`) margin.
    pub is_cross: bool,
    /// New leverage multiplier (`leverage`).
    pub leverage: u32,
}

/// USDC withdrawal off the L1: `{"type":"withdraw3"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlWithdrawAction {
    /// Destination address funds are withdrawn to (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// USDC transfer to another account: `{"type":"usdSend"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUsdSendAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Agent-wallet authorization: `{"type":"approveAgent"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlApproveAgentAction {
    /// Agent (API) wallet address being authorized (`agentAddress`).
    #[tsify(type = "string")]
    pub agent_address: Address,
    /// Optional human-readable agent name (`agentName`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub agent_name: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn order() -> HyperliquidCoreAction {
        HyperliquidCoreAction::Order(HlOrderAction {
            asset_index: 0,
            symbol: Some("BTC".to_owned()),
            is_buy: false,
            price: Decimal::new("60000"),
            size: Decimal::new("0.1"),
            reduce_only: false,
            tif: "gtc".to_owned(),
        })
    }

    /// `action_tag()` must equal the serde `action` discriminant for every
    /// variant — a policy trigger matches on the serde tag.
    #[test]
    fn action_tag_matches_serde() {
        let cases: Vec<HyperliquidCoreAction> = vec![
            order(),
            HyperliquidCoreAction::UpdateLeverage(HlUpdateLeverageAction {
                asset_index: 0,
                symbol: None,
                is_cross: true,
                leverage: 5,
            }),
            HyperliquidCoreAction::Withdraw(HlWithdrawAction {
                destination: Address::from([0x11; 20]),
                amount: Decimal::new("100"),
            }),
            HyperliquidCoreAction::UsdSend(HlUsdSendAction {
                destination: Address::from([0x22; 20]),
                amount: Decimal::new("50"),
            }),
            HyperliquidCoreAction::ApproveAgent(HlApproveAgentAction {
                agent_address: Address::from([0x33; 20]),
                agent_name: None,
            }),
        ];
        for c in cases {
            let json = serde_json::to_value(&c).unwrap();
            let serde_tag = json.get("action").and_then(serde_json::Value::as_str);
            assert_eq!(
                serde_tag,
                Some(c.action_tag()),
                "serde `action` tag must equal action_tag()"
            );
        }
    }

    /// Fractional price/size must round-trip (the whole reason we use `Decimal`,
    /// not `U256`, which rejects `"0.1"`).
    #[test]
    fn fractional_size_round_trips() {
        let json = serde_json::to_string(&order()).unwrap();
        assert!(
            json.contains("\"0.1\""),
            "fractional size preserved: {json}"
        );
        let back: HyperliquidCoreAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, order());
    }
}
