//! Hyperliquid L1 account state for one wallet.
//!
//! Wrapped by [`PositionKind::HyperliquidAccount`](super::PositionKind) and held
//! as exactly one [`Position`](super::Position) per wallet. Unlike
//! [`PerpPosition`](super::PerpPosition) (an EVM-ish position with a `U256`
//! `size_base`), every quantity here is a fractional-safe [`Decimal`] because a
//! Hyperliquid `/exchange` payload carries fractional sizes (`"0.1"`) natively.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, Decimal, Time};

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
    /// Perp-account equity / account value in USD from `marginSummary.accountValue`.
    /// Prefer this for portfolio totals; `perp_usdc` is the withdrawable/free
    /// margin balance and can understate an account with open positions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub perp_account_value_usd: Option<Decimal>,
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
    /// Today's day-start equity anchor for the daily-loss circuit breaker
    /// (`perp.equity_drawdown_bps`). Owned by [`Self::roll_equity_anchors`];
    /// the merge fns never touch it (anchor history must survive a failed
    /// poll). `None` until the first fresh equity observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub equity_baseline: Option<EquityAnchor>,
    /// Running high-water mark of `perp_account_value_usd` across all syncs,
    /// for the trailing max-drawdown circuit breaker. Same ownership rules as
    /// `equity_baseline`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub equity_hwm: Option<Decimal>,
    /// Bounded window of the account's most-recent fills (newest first) from
    /// `userFills`, for the behavioral session stats (`perp.session_fill_stats`:
    /// loss streak / trades today / realized `PnL`). Refreshed wholesale on the
    /// long-tail sync — each poll re-fetches the full window, so no
    /// cross-poll dedup is needed. Empty = never polled OR no fills in the
    /// window (the method treats both as "cannot serve").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fill_window: Vec<HlFillSummary>,
    /// Cumulative **non-funding capital flow** (USD, signed: deposits/transfers
    /// IN `+`, withdrawals/transfers OUT `−`) into the perp account since
    /// tracking started. Subtracted from raw equity by [`Self::roll_equity_anchors`]
    /// and `perp.equity_drawdown_bps` so a deposit/withdrawal does not read as
    /// profit/drawdown — the drawdown reflects trading P&L only (funding stays
    /// in equity, correctly counted as a cost). Server-owned, accumulated by the
    /// long-tail ledger sync; the merge fns ADD the per-poll delta and never
    /// wipe it (same survive-a-failed-poll rule as the anchors).
    #[serde(default = "zero_decimal")]
    pub cumulative_net_flow: Decimal,
    /// High-water `time` (unix ms) of the last `userNonFundingLedgerUpdates`
    /// entry folded into `cumulative_net_flow` — the dedup cursor so a re-fetched
    /// overlapping window never double-counts a flow. `0` = no ledger polled yet.
    #[serde(default)]
    pub ledger_cursor_ms: u64,
}

/// Serde default for `cumulative_net_flow` — `Decimal` has no `Default` (an empty
/// string is not a valid zero), so a legacy blob lacking the field reads as `"0"`.
fn zero_decimal() -> Decimal {
    Decimal::new("0")
}

impl Default for HlAccount {
    fn default() -> Self {
        Self {
            perp_usdc: None,
            perp_account_value_usd: None,
            pending_outflow: Decimal::new("0"),
            positions: Vec::new(),
            open_orders: Vec::new(),
            spot_balances: Vec::new(),
            staking: None,
            vault_equities: Vec::new(),
            borrow_lend: None,
            leverage_settings: Vec::new(),
            agents: Vec::new(),
            equity_baseline: None,
            equity_hwm: None,
            fill_window: Vec::new(),
            cumulative_net_flow: Decimal::new("0"),
            ledger_cursor_ms: 0,
        }
    }
}

/// One fill from the HL `userFills` feed, reduced to the session-stat fields.
///
/// `coin` is kept as the RAW venue symbol (perp name like `"BTC"` or a spot
/// pair like `"PURR/USDC"`/`"@107"`) — no asset-index resolution, so the
/// window never depends on the meta universe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlFillSummary {
    /// Venue-unique fill id (`tid`).
    pub tid: u64,
    /// Fill time, unix **milliseconds** (HL native).
    pub time: u64,
    /// Raw venue symbol (`coin`).
    pub coin: String,
    /// Signed realized `PnL` of this fill (`closedPnl`, USD); `"0.0"` on a
    /// pure open.
    pub closed_pnl: Decimal,
    /// Fill price (`px`).
    pub px: Decimal,
    /// Fill size in base units (`sz`).
    pub sz: Decimal,
}

/// A point-in-time equity anchor for drawdown measurement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EquityAnchor {
    /// Account equity (`marginSummary.accountValue`, unrealized `PnL`
    /// included) at the moment the anchor was taken.
    pub value: Decimal,
    /// When the anchor was taken (sync wall clock, UTC seconds).
    pub anchored_at: Time,
    /// `true` when the anchor approximates a true UTC day-open: the previous
    /// baseline was from the immediately-preceding UTC day, so we were watching
    /// across the rollover and the first tick of today (~15s cadence) took the
    /// anchor. `false` = watch started mid-day (anchor is watch-start equity).
    /// Residual corner: an outage that spans midnight and resumes intra-day
    /// still labels `true` while anchoring at the resume time — accepted; this
    /// is an honesty label, not a safety gate.
    pub trusted: bool,
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
// Independent domain flags: a per-domain freshness mask is the natural
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
    /// `fill_window` (`userFills`).
    pub fills: bool,
    /// `cumulative_net_flow` + `ledger_cursor_ms` (`userNonFundingLedgerUpdates`).
    /// When set, `merge_longtail` ADDS the carried per-poll flow delta and
    /// advances the cursor (unlike the other domains, which replace wholesale).
    pub ledger: bool,
}

impl HlAccount {
    /// Merge a freshly fetched **core** snapshot, overwriting only the domains
    /// marked fresh in `which` and **preserving** the rest (a failed/stale domain
    /// keeps its prior value). Long-tail fields and the reducer-owned
    /// `pending_outflow` are never touched.
    pub fn merge_core(&mut self, core: Self, which: CoreFresh) {
        if which.clearinghouse {
            self.perp_usdc = core.perp_usdc;
            self.perp_account_value_usd = core.perp_account_value_usd;
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

    /// Roll the equity anchors (day baseline + high-water mark) after a FRESH
    /// clearinghouse observation landed in `perp_account_value_usd`. Call only
    /// when the clearinghouse domain was authoritative this cycle — a stale or
    /// failed poll must not move the anchors.
    ///
    /// - HWM: monotone max of every observed equity (trailing max-drawdown).
    /// - Baseline: (re-)anchored on the first observation of each UTC day —
    ///   exactly where prop-firm daily-loss limits anchor (equity, day-start).
    ///
    /// Ordering parses the `Decimal` strings as `f64` — fine for comparison
    /// (a misorder needs values within ~1e-15 relative); the STORED value is
    /// always the exact venue string. The derived `Ord` on `Decimal` is
    /// lexicographic and must NOT be used here.
    pub fn roll_equity_anchors(&mut self, now: Time) {
        let Some(raw) = self.perp_account_value_usd.clone() else {
            return;
        };
        let Ok(raw_f) = raw.as_str().parse::<f64>() else {
            return;
        };
        if !raw_f.is_finite() {
            return;
        }
        // Flow-neutral equity: subtract the cumulative non-funding capital flow so
        // a deposit/withdrawal does not read as profit/drawdown — the drawdown must
        // reflect trading P&L only (funding stays in equity, correctly a cost).
        // `Decimal` has no arithmetic, so go through f64; but keep the EXACT venue
        // string when there is no flow (the common case + every flow-free test) and
        // only re-format when an adjustment is actually applied.
        let flow_f = self
            .cumulative_net_flow
            .as_str()
            .parse::<f64>()
            .unwrap_or(0.0);
        let (cur_f, current) = if flow_f == 0.0 || !flow_f.is_finite() {
            (raw_f, raw)
        } else {
            let adjusted = raw_f - flow_f;
            (adjusted, Decimal::new(format!("{adjusted}")))
        };

        // HWM: monotone max. An unparseable stored HWM (corrupt blob) heals to
        // the current observation rather than wedging forever.
        let hwm_f = self
            .equity_hwm
            .as_ref()
            .and_then(|h| h.as_str().parse::<f64>().ok());
        if hwm_f.is_none_or(|h| cur_f > h) {
            self.equity_hwm = Some(current.clone());
        }

        // Day baseline: first observation of each UTC day re-anchors.
        let today = utc_day(now);
        let prev_day = self
            .equity_baseline
            .as_ref()
            .map(|b| utc_day(b.anchored_at));
        if prev_day != Some(today) {
            self.equity_baseline = Some(EquityAnchor {
                value: current,
                anchored_at: now,
                // Trusted ⇔ we held yesterday's baseline, i.e. we were watching
                // across the rollover (see `EquityAnchor::trusted` docs).
                trusted: prev_day.is_some_and(|d| d + 1 == today),
            });
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
        if which.fills {
            // Wholesale replace: every poll re-fetches the full recency
            // window, so the fetched vec IS the new window (no union/prune).
            self.fill_window = lt.fill_window;
        }
        if which.ledger {
            // ACCUMULATE (unlike the other domains): `lt.cumulative_net_flow`
            // carries the per-poll delta of flows newer than our cursor, so we
            // add it onto the running total and advance the cursor. `Decimal`
            // has no arithmetic → sum via f64 (USD cents are well within f64).
            let acc = self
                .cumulative_net_flow
                .as_str()
                .parse::<f64>()
                .unwrap_or(0.0);
            let delta = lt
                .cumulative_net_flow
                .as_str()
                .parse::<f64>()
                .unwrap_or(0.0);
            self.cumulative_net_flow = Decimal::new(format!("{}", acc + delta));
            self.ledger_cursor_ms = lt.ledger_cursor_ms;
        }
    }
}

/// UTC day index (days since the epoch) — the rollover boundary for the daily
/// equity baseline.
const fn utc_day(t: Time) -> u64 {
    t.as_unix() / 86_400
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

    fn fill(tid: u64) -> HlFillSummary {
        HlFillSummary {
            tid,
            time: 1_781_187_103_047,
            coin: "BTC".to_owned(),
            closed_pnl: Decimal::new("-1.5"),
            px: Decimal::new("60000"),
            sz: Decimal::new("0.1"),
        }
    }

    #[test]
    fn merge_longtail_replaces_fill_window_only_when_fresh() {
        let mut persisted = acct("1");
        persisted.fill_window = vec![fill(1), fill(2)];

        // Failed fills poll (mask false) → window preserved.
        persisted.merge_longtail(HlAccount::default(), LongtailFresh::default());
        assert_eq!(persisted.fill_window.len(), 2);

        // Fresh poll → wholesale replace (even shrinking is authoritative —
        // the fetched vec IS the full recency window).
        let lt = HlAccount {
            fill_window: vec![fill(3)],
            ..Default::default()
        };
        persisted.merge_longtail(
            lt,
            LongtailFresh {
                fills: true,
                ..Default::default()
            },
        );
        assert_eq!(persisted.fill_window.len(), 1);
        assert_eq!(persisted.fill_window[0].tid, 3);
    }

    #[test]
    fn merge_longtail_accumulates_ledger_flow_and_advances_cursor() {
        let mut persisted = HlAccount {
            cumulative_net_flow: Decimal::new("-200"),
            ledger_cursor_ms: 1_000,
            ..Default::default()
        };
        // Fresh ledger poll carries the per-poll DELTA (−50) and the new cursor.
        let lt = HlAccount {
            cumulative_net_flow: Decimal::new("-50"),
            ledger_cursor_ms: 2_000,
            ..Default::default()
        };
        persisted.merge_longtail(
            lt,
            LongtailFresh {
                ledger: true,
                ..Default::default()
            },
        );
        // Delta is ADDED (not replaced): −200 + −50 = −250.
        assert_eq!(persisted.cumulative_net_flow, Decimal::new("-250"));
        assert_eq!(persisted.ledger_cursor_ms, 2_000);
    }

    #[test]
    fn merge_longtail_preserves_ledger_when_not_fresh() {
        let mut persisted = HlAccount {
            cumulative_net_flow: Decimal::new("-200"),
            ledger_cursor_ms: 1_000,
            ..Default::default()
        };
        // Failed ledger poll (mask false) → flow + cursor preserved, never wiped.
        persisted.merge_longtail(
            HlAccount {
                cumulative_net_flow: Decimal::new("999"),
                ledger_cursor_ms: 9_999,
                ..Default::default()
            },
            LongtailFresh::default(),
        );
        assert_eq!(persisted.cumulative_net_flow, Decimal::new("-200"));
        assert_eq!(persisted.ledger_cursor_ms, 1_000);
    }
}

#[cfg(test)]
mod anchor_tests {
    use super::*;

    const DAY: u64 = 86_400;

    fn acct_with_equity(equity: &str) -> HlAccount {
        HlAccount {
            perp_account_value_usd: Some(Decimal::new(equity)),
            ..Default::default()
        }
    }

    #[test]
    fn first_observation_anchors_untrusted_and_seeds_hwm() {
        // Mid-day watch start: the anchor is watch-start equity, NOT day-open.
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        let b = a.equity_baseline.as_ref().expect("baseline set");
        assert_eq!(b.value, Decimal::new("1000"));
        assert!(!b.trusted, "first-ever anchor is mid-day → untrusted");
        assert_eq!(a.equity_hwm, Some(Decimal::new("1000")));
    }

    #[test]
    fn same_day_keeps_baseline_and_raises_hwm_monotonically() {
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        // Later the same day: equity pumps → HWM up, baseline UNCHANGED.
        a.perp_account_value_usd = Some(Decimal::new("1200"));
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 7200));
        assert_eq!(
            a.equity_baseline.as_ref().unwrap().value,
            Decimal::new("1000")
        );
        assert_eq!(a.equity_hwm, Some(Decimal::new("1200")));
        // Then dumps → HWM must NOT come down.
        a.perp_account_value_usd = Some(Decimal::new("900"));
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 10_800));
        assert_eq!(a.equity_hwm, Some(Decimal::new("1200")));
        assert_eq!(
            a.equity_baseline.as_ref().unwrap().value,
            Decimal::new("1000")
        );
    }

    #[test]
    fn hwm_comparison_is_numeric_not_lexicographic() {
        // "1200" > "999" numerically but "1200" < "999" as strings — the
        // derived (lexicographic) Ord on Decimal must not be what rolls HWM.
        let mut a = acct_with_equity("999");
        a.roll_equity_anchors(Time::from_unix(10 * DAY));
        a.perp_account_value_usd = Some(Decimal::new("1200"));
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 60));
        assert_eq!(a.equity_hwm, Some(Decimal::new("1200")));
    }

    #[test]
    fn next_day_rollover_reanchors_trusted() {
        // Watching across the UTC rollover: yesterday's baseline exists →
        // today's first tick is ≈ day-open → trusted.
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        a.perp_account_value_usd = Some(Decimal::new("950"));
        a.roll_equity_anchors(Time::from_unix(11 * DAY + 15));
        let b = a.equity_baseline.as_ref().unwrap();
        assert_eq!(b.value, Decimal::new("950"));
        assert!(b.trusted, "previous baseline from yesterday → trusted");
        // HWM survives the rollover (trailing, not daily).
        assert_eq!(a.equity_hwm, Some(Decimal::new("1000")));
    }

    #[test]
    fn multi_day_gap_reanchors_untrusted() {
        // Server (or registration) was dark all of yesterday: the anchor is
        // NOT a day-open observation.
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        a.perp_account_value_usd = Some(Decimal::new("800"));
        a.roll_equity_anchors(Time::from_unix(12 * DAY + 15));
        let b = a.equity_baseline.as_ref().unwrap();
        assert_eq!(b.value, Decimal::new("800"));
        assert!(!b.trusted, "gap spanned a full day → untrusted");
    }

    #[test]
    fn no_equity_is_a_noop() {
        let mut a = HlAccount::default();
        a.roll_equity_anchors(Time::from_unix(10 * DAY));
        assert!(a.equity_baseline.is_none());
        assert!(a.equity_hwm.is_none());
    }

    #[test]
    fn unparseable_equity_is_a_noop() {
        let mut a = acct_with_equity("not-a-number");
        a.roll_equity_anchors(Time::from_unix(10 * DAY));
        assert!(a.equity_baseline.is_none());
        assert!(a.equity_hwm.is_none());
    }

    #[test]
    fn legacy_json_without_anchor_fields_deserializes_to_none() {
        // Backward compat with persisted JSONB blobs written before the
        // anchor fields existed.
        let legacy = serde_json::json!({
            "pending_outflow": "0",
            "positions": [],
            "open_orders": [],
            "leverage_settings": [],
            "agents": []
        });
        let acct: HlAccount = serde_json::from_value(legacy).expect("legacy blob");
        assert!(acct.equity_baseline.is_none());
        assert!(acct.equity_hwm.is_none());
        // Flow-reconciliation fields are also serde-default on a legacy blob.
        assert_eq!(acct.cumulative_net_flow, Decimal::new("0"));
        assert_eq!(acct.ledger_cursor_ms, 0);
    }

    #[test]
    fn deposit_does_not_inflate_hwm() {
        // Anchor at equity 1000 (no flow yet) → HWM 1000.
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        assert_eq!(a.equity_hwm, Some(Decimal::new("1000")));
        // Deposit $500: raw equity jumps to 1500 and the ledger sync records a
        // +500 cumulative flow. Flow-neutral equity is 1500 − 500 = 1000, so the
        // HWM must NOT rise — a deposit is not a trading gain (and must not be
        // allowed to erase a real drawdown / reset the circuit breaker).
        a.perp_account_value_usd = Some(Decimal::new("1500"));
        a.cumulative_net_flow = Decimal::new("500");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 7200));
        assert_eq!(
            a.equity_hwm,
            Some(Decimal::new("1000")),
            "deposit must not raise HWM"
        );
    }

    #[test]
    fn withdrawal_baseline_reanchors_on_flow_neutral_equity() {
        // Day 10 baseline at 1000 (no flow).
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        // Next day, after a $200 withdrawal: raw equity 800, ledger flow −200.
        // The day-start baseline must re-anchor on flow-neutral equity
        // (800 − (−200) = 1000), NOT raw 800 — else the withdrawal reads as a
        // 20% daily loss.
        a.perp_account_value_usd = Some(Decimal::new("800"));
        a.cumulative_net_flow = Decimal::new("-200");
        a.roll_equity_anchors(Time::from_unix(11 * DAY + 60));
        assert_eq!(
            a.equity_baseline.as_ref().unwrap().value,
            Decimal::new("1000"),
            "baseline anchors on flow-neutral equity, not raw post-withdrawal 800"
        );
    }

    #[test]
    fn flow_neutral_still_captures_real_trading_loss() {
        // After a $200 withdrawal (flow −200), a genuine trading loss must still
        // register. Raw 250 with flow −200 → flow-neutral 450; against a 1000
        // peak that is a real 55% drawdown the breaker should see.
        let mut a = acct_with_equity("1000");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 3600));
        a.perp_account_value_usd = Some(Decimal::new("250"));
        a.cumulative_net_flow = Decimal::new("-200");
        a.roll_equity_anchors(Time::from_unix(10 * DAY + 7200));
        // HWM stays at the flow-neutral peak 1000 (450 < 1000, no new high).
        assert_eq!(a.equity_hwm, Some(Decimal::new("1000")));
        // Baseline (same day) unchanged at 1000.
        assert_eq!(
            a.equity_baseline.as_ref().unwrap().value,
            Decimal::new("1000")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl_account_round_trips_through_json() {
        let acct = HlAccount {
            perp_usdc: Some(Decimal::new("1000.5")),
            perp_account_value_usd: Some(Decimal::new("1000.5")),
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
            equity_baseline: Some(EquityAnchor {
                value: Decimal::new("1000.5"),
                anchored_at: Time::from_unix(1_735_430_400),
                trusted: true,
            }),
            equity_hwm: Some(Decimal::new("1100.25")),
            fill_window: vec![HlFillSummary {
                tid: 533_471_271_655_943,
                time: 1_781_187_103_047,
                coin: "GMX".to_owned(),
                closed_pnl: Decimal::new("0.080665"),
                px: Decimal::new("5.4112"),
                sz: Decimal::new("3.65"),
            }],
            cumulative_net_flow: Decimal::new("-150.5"),
            ledger_cursor_ms: 1_781_187_103_047,
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
        assert_eq!(acct.cumulative_net_flow, Decimal::new("0"));
        assert_eq!(acct.ledger_cursor_ms, 0);
    }
}
