//! Hyperliquid L1 account state for one wallet.
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
/// `Default` is implemented manually because [`Decimal`] does not derive
/// `Default`; the explicit implementation keeps `pending_outflow` at `"0"` and
/// `perp_usdc` at `None` until a balance is synced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlAccount {
    /// Perp-account USDC margin balance. Moves on withdraw / `usd_send` / fills.
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
    /// Spot-account token balances from `spotClearinghouseState`.
    #[serde(default)]
    pub spot_balances: Vec<HlSpotBalance>,
    /// Staking-account primitive state from `delegatorSummary` + `delegations`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub staking: Option<HlStakingAccount>,
    /// Vault deposit equities from `userVaultEquities`.
    #[serde(default)]
    pub vault_equities: Vec<HlVaultEquity>,
    /// Borrow/lend primitive state from `borrowLendUserState`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub borrow_lend: Option<HlBorrowLendAccount>,
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
            spot_balances: Vec::new(),
            staking: None,
            vault_equities: Vec::new(),
            borrow_lend: None,
            leverage_settings: Vec::new(),
            agents: Vec::new(),
        }
    }
}

/// Which **core** domains in a freshly fetched snapshot are authoritative.
///
/// A domain is authoritative when its fetch + parse both succeeded this cycle.
/// Domains left `false` are *preserved* from the existing account by
/// [`HlAccount::merge_core`] — a failed poll must never wipe good state
/// (resilience: failed fields keep their prior value).
#[derive(Clone, Copy, Debug, Default)]
pub struct CoreFresh {
    /// `perp_usdc` + `positions` + `leverage_settings` (one `clearinghouseState`).
    pub clearinghouse: bool,
    /// `open_orders` (separate `openOrders` call).
    pub open_orders: bool,
    /// `spot_balances` (separate `spotClearinghouseState` call).
    pub spot: bool,
}

/// Which **long-tail** domains are authoritative this cycle. Same
/// preserve-on-miss semantics as [`CoreFresh`] (see [`HlAccount::merge_longtail`]).
// Four independent domain flags: a per-domain freshness mask is the natural
// representation here, so the >3-bools lint does not apply.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LongtailFresh {
    /// `staking` (`delegatorSummary` + `delegations`, both required).
    pub staking: bool,
    /// `vault_equities` (`userVaultEquities`).
    pub vault_equities: bool,
    /// `borrow_lend` (`borrowLendUserState`).
    pub borrow_lend: bool,
    /// `agents` (delegated agent wallets).
    pub agents: bool,
}

impl HlAccount {
    /// Merge a freshly fetched **core** snapshot, overwriting only the domains
    /// marked fresh in `which` and **preserving** the rest (a failed/stale domain
    /// keeps its prior value). Long-tail fields and the reducer-owned
    /// `pending_outflow` are never touched.
    pub fn merge_core(&mut self, core: Self, which: CoreFresh) {
        if which.clearinghouse {
            self.perp_usdc = core.perp_usdc;
            self.positions = core.positions;
            self.leverage_settings = core.leverage_settings;
        }
        if which.open_orders {
            self.open_orders = core.open_orders;
        }
        if which.spot {
            self.spot_balances = core.spot_balances;
        }
    }

    /// Merge freshly fetched **long-tail** fields, overwriting only the domains
    /// marked fresh in `which` and preserving the rest. Core fields and
    /// `pending_outflow` are never touched.
    pub fn merge_longtail(&mut self, lt: Self, which: LongtailFresh) {
        if which.staking {
            self.staking = lt.staking;
        }
        if which.vault_equities {
            self.vault_equities = lt.vault_equities;
        }
        if which.borrow_lend {
            self.borrow_lend = lt.borrow_lend;
        }
        if which.agents {
            self.agents = lt.agents;
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

/// A spot-account token balance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSpotBalance {
    /// Human-readable token symbol from Hyperliquid, e.g. `"USDC"` or `"HYPE"`.
    pub coin: String,
    /// `HyperCore` token index.
    pub token: u32,
    /// Total token balance.
    pub total: Decimal,
    /// Amount reserved by open spot orders or maintenance constraints.
    pub hold: Decimal,
    /// Venue-provided entry notional for the token balance.
    pub entry_ntl: Decimal,
    /// Available balance after maintenance, keyed by token index when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub available_after_maintenance: Option<Decimal>,
}

/// A staking-account snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlStakingAccount {
    /// HYPE currently delegated to validators.
    pub delegated: Decimal,
    /// HYPE in the staking account but not delegated.
    pub undelegated: Decimal,
    /// HYPE queued for withdrawal from staking.
    pub total_pending_withdrawal: Decimal,
    /// Number of pending withdrawal entries.
    pub n_pending_withdrawals: u32,
    /// Per-validator delegation positions.
    pub delegations: Vec<HlStakingDelegation>,
}

/// A single validator delegation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlStakingDelegation {
    /// Validator address.
    #[tsify(type = "string")]
    pub validator: Address,
    /// Delegated HYPE amount.
    pub amount: Decimal,
    /// Millisecond timestamp until which the delegation is locked, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub locked_until_timestamp: Option<u64>,
}

/// A user's equity in a vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlVaultEquity {
    /// Vault address.
    #[tsify(type = "string")]
    pub vault_address: Address,
    /// Venue-reported user equity in the vault.
    pub equity: Decimal,
    /// Millisecond timestamp at which the equity unlocks, if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub locked_until_timestamp: Option<u64>,
}

/// Borrow/lend account primitives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlBorrowLendAccount {
    /// Per-token borrow/supply state.
    pub token_states: Vec<HlBorrowLendTokenState>,
    /// Venue health string, e.g. `"healthy"`, when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub health: Option<String>,
    /// Venue health factor, if non-null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub health_factor: Option<Decimal>,
}

/// Borrow/lend state for one token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlBorrowLendTokenState {
    /// `HyperCore` token index.
    pub token: u32,
    /// Borrow primitive.
    pub borrow: HlBorrowLendBalance,
    /// Supply primitive.
    pub supply: HlBorrowLendBalance,
}

/// Borrow/lend side amounts for one token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlBorrowLendBalance {
    /// Principal/basis amount.
    pub basis: Decimal,
    /// Present value amount.
    pub value: Decimal,
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
mod merge_tests {
    use super::*;

    fn acct(tag: &str) -> HlAccount {
        HlAccount {
            perp_usdc: Some(Decimal::new(tag)),
            pending_outflow: Decimal::new("42"),
            ..Default::default()
        }
    }

    fn spot_balance(coin: &str) -> HlSpotBalance {
        HlSpotBalance {
            coin: coin.to_string(),
            token: 0,
            total: Decimal::new("1"),
            hold: Decimal::new("0"),
            entry_ntl: Decimal::new("0"),
            available_after_maintenance: None,
        }
    }

    fn staking_acct() -> HlStakingAccount {
        HlStakingAccount {
            delegated: Decimal::new("5"),
            undelegated: Decimal::new("0"),
            total_pending_withdrawal: Decimal::new("0"),
            n_pending_withdrawals: 0,
            delegations: Vec::new(),
        }
    }

    #[test]
    fn merge_core_updates_fresh_preserves_stale() {
        let mut persisted = acct("1");
        persisted.spot_balances = vec![spot_balance("USDC")];
        // clearinghouse + open_orders fresh; spot's fetch FAILED → not fresh.
        let fresh = acct("2"); // perp_usdc = 2, spot_balances empty
        persisted.merge_core(
            fresh,
            CoreFresh {
                clearinghouse: true,
                open_orders: true,
                spot: false,
            },
        );
        assert_eq!(persisted.perp_usdc, Some(Decimal::new("2"))); // fresh → updated
        assert!(!persisted.spot_balances.is_empty()); // stale → preserved
        assert_eq!(persisted.pending_outflow, Decimal::new("42")); // reducer field kept
    }

    #[test]
    fn merge_core_all_stale_preserves_everything() {
        // Regression: a clearinghouse/meta blip yields an all-false mask, which
        // must NOT wipe good persisted state to defaults.
        let mut persisted = acct("100");
        persisted.merge_core(HlAccount::default(), CoreFresh::default());
        assert_eq!(persisted.perp_usdc, Some(Decimal::new("100"))); // not wiped to None
    }

    #[test]
    fn merge_longtail_preserves_stale_and_keeps_core() {
        let mut persisted = acct("1");
        persisted.staking = Some(staking_acct());
        // every long-tail fetch failed this cycle → all-false mask.
        persisted.merge_longtail(HlAccount::default(), LongtailFresh::default());
        assert!(persisted.staking.is_some()); // stale long-tail preserved
        assert_eq!(persisted.perp_usdc, Some(Decimal::new("1"))); // core untouched
    }

    #[test]
    fn merge_longtail_updates_fresh_domain() {
        let mut persisted = acct("1"); // staking None
        let lt = HlAccount {
            staking: Some(staking_acct()),
            ..Default::default()
        };
        persisted.merge_longtail(
            lt,
            LongtailFresh {
                staking: true,
                ..Default::default()
            },
        );
        assert!(persisted.staking.is_some()); // fresh staking written in
    }
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
            spot_balances: vec![HlSpotBalance {
                coin: "USDC".to_owned(),
                token: 0,
                total: Decimal::new("1125.961894"),
                hold: Decimal::new("1077.497057"),
                entry_ntl: Decimal::new("0.0"),
                available_after_maintenance: Some(Decimal::new("48.464837")),
            }],
            staking: Some(HlStakingAccount {
                delegated: Decimal::new("0.0"),
                undelegated: Decimal::new("0.0"),
                total_pending_withdrawal: Decimal::new("46.84529183"),
                n_pending_withdrawals: 1,
                delegations: vec![HlStakingDelegation {
                    validator: Address::from([0x22; 20]),
                    amount: Decimal::new("47.0"),
                    locked_until_timestamp: Some(1_735_466_781_353_u64),
                }],
            }),
            vault_equities: vec![HlVaultEquity {
                vault_address: Address::from([0x33; 20]),
                equity: Decimal::new("742500.082809"),
                locked_until_timestamp: Some(1_741_132_800_000_u64),
            }],
            borrow_lend: Some(HlBorrowLendAccount {
                token_states: vec![HlBorrowLendTokenState {
                    token: 0,
                    borrow: HlBorrowLendBalance {
                        basis: Decimal::new("0.0"),
                        value: Decimal::new("0.0"),
                    },
                    supply: HlBorrowLendBalance {
                        basis: Decimal::new("44.69295862"),
                        value: Decimal::new("44.69692314"),
                    },
                }],
                health: Some("healthy".to_owned()),
                health_factor: None,
            }),
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
        assert!(acct.spot_balances.is_empty());
        assert!(acct.staking.is_none());
        assert!(acct.vault_equities.is_empty());
        assert!(acct.borrow_lend.is_none());
        assert!(acct.leverage_settings.is_empty());
        assert!(acct.agents.is_empty());
    }
}
