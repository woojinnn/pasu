//! Hyperliquid REST fetcher — mark price, funding rate, open orders, account primitives.
//! API: <https://api.hyperliquid.xyz/info>  (POST JSON body)
//! - `hl_spot_account`     → spot token balances
//! - `hl_staking_summary`  → staking summary
//! - `hl_vault_equities`   → per-vault user equity
//! - `hl_borrow_lend`      → borrow/lend user state
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal as RustDecimal;
use serde_json::{json, Value};

use policy_state::position::{
    CoreFresh, HlAccount, HlAgentApproval, HlBorrowLendAccount, HlBorrowLendBalance,
    HlBorrowLendTokenState, HlLeverageSetting, HlOpenOrder, HlPosition, HlSpotBalance,
    HlStakingAccount, HlStakingDelegation, HlVaultEquity, LongtailFresh,
};
use policy_state::primitives::Time;
use policy_state::{Address, DataSource, Decimal, MarketRef, VenueRef, U256};
use policy_transition::action::perp::PerpAccountState;

use crate::config::HyperliquidConfig;
use crate::error::SyncError;
use crate::walker::{ActionSlot, FieldLocation};

use super::ttl_cache::TtlCache;

pub const HL_API_BASE: &str = "https://api.hyperliquid.xyz";

pub struct HyperliquidFetcher {
    client: reqwest::Client,
    base_url: String,
    meta_ttl_secs: u64,
    meta_cache: Mutex<TtlCache<String, Value>>, // key = dex ("" = native)
    dexs_cache: Mutex<TtlCache<(), Vec<String>>>, // perpDexs list
}

impl Default for HyperliquidFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperliquidFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self::with_base_url(HL_API_BASE.to_string())
    }

    #[must_use]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url,
            meta_ttl_secs: 600,
            meta_cache: Mutex::new(TtlCache::new()),
            dexs_cache: Mutex::new(TtlCache::new()),
        }
    }

    #[must_use]
    pub fn from_sync_config(cfg: &HyperliquidConfig) -> Self {
        let mut fetcher = Self::with_base_url(cfg.endpoint.clone());
        fetcher.meta_ttl_secs = cfg.meta_ttl_secs;
        fetcher
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Effective Hyperliquid `/info` endpoint for this fetcher.
    #[must_use]
    pub fn info_endpoint(&self) -> String {
        self.info_url("")
    }

    pub async fn fetch(&self, source: &DataSource) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = match source {
            DataSource::VenueApi {
                endpoint,
                parser_id,
                ..
            } => (endpoint.clone(), parser_id.clone()),
            _ => {
                return Err(SyncError::FetchFailed {
                    source_id: "hyperliquid".into(),
                    reason: "not a VenueApi source".into(),
                });
            }
        };

        self.fetch_payload_for_parser(&endpoint, &parser_id, None, None)
            .await
    }

    pub async fn fetch_meta(&self, endpoint: &str) -> Result<Value, SyncError> {
        self.fetch_meta_for_dex(endpoint, endpoint_dex(endpoint).as_deref())
            .await
    }

    /// `meta` for a dex, cached for `meta_ttl_secs`. `now` is injected by the caller.
    pub async fn cached_meta(&self, endpoint: &str, now: Time) -> Result<Value, SyncError> {
        let key = endpoint_dex(endpoint).unwrap_or_default();
        let cached = self
            .meta_cache
            .lock()
            .unwrap()
            .get(&key, now, self.meta_ttl_secs);
        if let Some(v) = cached {
            return Ok(v);
        }
        let v = self.fetch_meta(endpoint).await?;
        self.meta_cache.lock().unwrap().put(key, v.clone(), now);
        Ok(v)
    }

    /// perpDexs list, cached for `meta_ttl_secs`.
    pub async fn cached_perp_dexs(
        &self,
        endpoint: &str,
        now: Time,
    ) -> Result<Vec<String>, SyncError> {
        let cached = self
            .dexs_cache
            .lock()
            .unwrap()
            .get(&(), now, self.meta_ttl_secs);
        if let Some(v) = cached {
            return Ok(v);
        }
        let v = self.fetch_perp_dexs(endpoint).await?;
        self.dexs_cache.lock().unwrap().put((), v.clone(), now);
        Ok(v)
    }

    /// Best-effort **core** fetch for the **native** dex (no perp-dex fan-out).
    /// Returns `(account, fresh, errors)`; `fresh` tells the caller which core
    /// domains to overwrite vs preserve.
    pub async fn fetch_hl_core(
        &self,
        endpoint: &str,
        user: &Address,
        now: Time,
    ) -> (HlAccount, CoreFresh, Vec<String>) {
        let meta = match self.cached_meta(endpoint, now).await {
            Ok(m) => m,
            Err(e) => {
                return (
                    HlAccount::default(),
                    CoreFresh::default(),
                    vec![format!("meta: {e}")],
                );
            }
        };
        let clearinghouse = self.fetch_clearinghouse_state(endpoint, user).await;
        let spot = self.fetch_spot_clearinghouse_state(endpoint, user).await;
        let open_orders = self.fetch_open_orders(endpoint, user).await;
        assemble_core(clearinghouse, spot, open_orders, &meta)
    }

    /// Best-effort **long-tail** fetch (staking / vaults / borrow-lend / agents).
    /// Returns `(account, fresh, errors)`; `fresh` tells the caller which long-tail
    /// domains to overwrite vs preserve.
    pub async fn fetch_hl_longtail(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> (HlAccount, LongtailFresh, Vec<String>) {
        let staking = self.fetch_delegator_summary(endpoint, user).await;
        let delegations = self.fetch_delegations(endpoint, user).await;
        let vaults = self.fetch_user_vault_equities(endpoint, user).await;
        let borrow = self.fetch_borrow_lend_user_state(endpoint, user).await;
        let agents = self.fetch_agents(endpoint, user).await;
        assemble_longtail(staking, delegations, vaults, borrow, agents)
    }

    pub async fn fetch_clearinghouse_state(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_clearinghouse_state_for_dex(endpoint, user, endpoint_dex(endpoint).as_deref())
            .await
    }

    async fn fetch_clearinghouse_state_for_dex(
        &self,
        endpoint: &str,
        user: &Address,
        dex: Option<&str>,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            with_dex(
                json!({ "type": "clearinghouseState", "user": hl_user(user) }),
                dex,
            ),
        )
        .await
    }

    pub async fn fetch_open_orders(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_open_orders_for_dex(endpoint, user, endpoint_dex(endpoint).as_deref())
            .await
    }

    async fn fetch_open_orders_for_dex(
        &self,
        endpoint: &str,
        user: &Address,
        dex: Option<&str>,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            with_dex(
                json!({ "type": "frontendOpenOrders", "user": hl_user(user) }),
                dex,
            ),
        )
        .await
    }

    pub async fn fetch_agents(&self, endpoint: &str, user: &Address) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "extraAgents", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_spot_clearinghouse_state(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "spotClearinghouseState", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_delegator_summary(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "delegatorSummary", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_delegations(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "delegations", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_user_vault_equities(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "userVaultEquities", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_borrow_lend_user_state(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "borrowLendUserState", "user": hl_user(user) }),
        )
        .await
    }

    async fn fetch_meta_for_dex(
        &self,
        endpoint: &str,
        dex: Option<&str>,
    ) -> Result<Value, SyncError> {
        self.fetch_info(endpoint, with_dex(json!({ "type": "meta" }), dex))
            .await
    }

    async fn fetch_perp_dexs(&self, endpoint: &str) -> Result<Vec<String>, SyncError> {
        let payload = self
            .fetch_info(endpoint, json!({ "type": "perpDexs" }))
            .await?;
        let Some(items) = payload.as_array() else {
            return Ok(Vec::new());
        };
        Ok(items
            .iter()
            .filter_map(|item| value_at(item, &["name"]).and_then(Value::as_str))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub async fn fetch_account_snapshot(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<HlAccount, SyncError> {
        let agents = self.fetch_agents(endpoint, user).await?;
        let configured_dex = endpoint_dex(endpoint);
        let mut account = self
            .fetch_account_snapshot_for_dex(endpoint, user, configured_dex.as_deref(), &agents)
            .await?;

        if configured_dex.is_none() {
            for dex in self.fetch_perp_dexs(endpoint).await? {
                let extra = self
                    .fetch_account_snapshot_for_dex(
                        endpoint,
                        user,
                        Some(&dex),
                        &Value::Array(Vec::new()),
                    )
                    .await?;
                merge_hl_account(&mut account, extra);
            }
        }

        let spot = self.fetch_spot_clearinghouse_state(endpoint, user).await?;
        let staking_summary = self.fetch_delegator_summary(endpoint, user).await?;
        let delegations = self.fetch_delegations(endpoint, user).await?;
        let vault_equities = self.fetch_user_vault_equities(endpoint, user).await?;
        let borrow_lend = self.fetch_borrow_lend_user_state(endpoint, user).await?;
        apply_hl_account_primitives(
            &mut account,
            &spot,
            &staking_summary,
            &delegations,
            &vault_equities,
            &borrow_lend,
        )?;

        Ok(account)
    }

    async fn fetch_account_snapshot_for_dex(
        &self,
        endpoint: &str,
        user: &Address,
        dex: Option<&str>,
        agents: &Value,
    ) -> Result<HlAccount, SyncError> {
        let clearinghouse = self
            .fetch_clearinghouse_state_for_dex(endpoint, user, dex)
            .await?;
        let open_orders = self.fetch_open_orders_for_dex(endpoint, user, dex).await?;
        let meta = self.fetch_meta_for_dex(endpoint, dex).await?;
        parse_account_snapshot(&clearinghouse, &open_orders, agents, &meta)
    }

    pub async fn fetch_action_value(
        &self,
        source: &DataSource,
        slot: &ActionSlot,
        market_symbol: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = venue_source_parts(source)?;
        let payload = self
            .fetch_payload_for_parser(endpoint, parser_id, Some(user), Some(market_symbol))
            .await?;
        parse_live_input_value(parser_id, slot, market_symbol, &payload)
    }

    pub async fn fetch_state_value(
        &self,
        source: &DataSource,
        location: &FieldLocation,
        market_symbol: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = venue_source_parts(source)?;
        let payload = self
            .fetch_payload_for_parser(endpoint, parser_id, Some(user), Some(market_symbol))
            .await?;
        parse_state_value(parser_id, location, market_symbol, &payload)
    }

    async fn fetch_payload_for_parser(
        &self,
        endpoint: &str,
        parser_id: &str,
        user: Option<&Address>,
        market_symbol: Option<&str>,
    ) -> Result<Value, SyncError> {
        let zero_user = Address::ZERO;
        let user = user.unwrap_or(&zero_user);
        let body = payload_for_parser(endpoint, parser_id, user, market_symbol)?;
        self.fetch_info(endpoint, body).await
    }

    async fn fetch_info(&self, endpoint: &str, body: Value) -> Result<Value, SyncError> {
        let url = self.info_url(endpoint);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("http: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("status {}", resp.status()),
            });
        }

        let value: Value = resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: format!("json: {e}"),
        })?;
        Ok(value)
    }

    fn info_url(&self, endpoint: &str) -> String {
        let raw = if endpoint.is_empty() {
            if self.base_url.is_empty() {
                HL_API_BASE
            } else {
                self.base_url.as_str()
            }
        } else {
            endpoint
        };
        if raw.contains("/info") {
            raw.to_owned()
        } else {
            format!("{}/info", raw.trim_end_matches('/'))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HlAssetMetric {
    MarkPrice,
    OraclePrice,
    Funding,
    OpenInterest,
    MaxLeverage,
    InitialMarginBp,
    MaintenanceMarginBp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FeeSide {
    Maker,
    Taker,
}

/// Assemble the **core** `HlAccount` from already-fetched parts, best-effort.
/// `clearinghouse` is the anchor (no `perp_usdc` / positions / leverage without
/// it); `spot` and `open_orders` are additive. Returns `(account, fresh, errors)`
/// where `fresh` marks exactly the domains that fetched **and** parsed OK, so the
/// caller's `merge_core` overwrites only those and preserves the rest.
pub(crate) fn assemble_core(
    clearinghouse: Result<Value, SyncError>,
    spot: Result<Value, SyncError>,
    open_orders: Result<Value, SyncError>,
    meta: &Value,
) -> (HlAccount, CoreFresh, Vec<String>) {
    let mut errors = Vec::new();
    let mut fresh = CoreFresh::default();
    let empty_agents = Value::Array(Vec::new());

    let clearinghouse = match clearinghouse {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("clearinghouse: {e}"));
            // No anchor → all-false mask → caller preserves all core fields.
            return (HlAccount::default(), fresh, errors);
        }
    };
    let orders_val = match open_orders {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("open_orders: {e}"));
            Value::Array(Vec::new())
        }
    };
    let mut account = match parse_account_snapshot(&clearinghouse, &orders_val, &empty_agents, meta)
    {
        Ok(a) => {
            // Native dex bundle parsed → native is fresh. (Task 4 generalizes this
            // to per-builder-dex fan-out via `assemble_core_dex`.)
            fresh.fresh_dexs.push(None);
            a
        }
        Err(e) => {
            errors.push(format!("parse core: {e}"));
            return (HlAccount::default(), CoreFresh::default(), errors);
        }
    };
    match spot {
        Ok(v) => match parse_hl_spot_balances(&v) {
            Ok(b) => {
                account.spot_balances = b;
                fresh.spot = true;
            }
            Err(e) => errors.push(format!("spot parse: {e}")),
        },
        Err(e) => errors.push(format!("spot: {e}")),
    }
    (account, fresh, errors)
}

/// Assemble the **long-tail** fields best-effort. Each domain is independent; a
/// failed/unparseable one is recorded and left unmarked. Returns
/// `(account, fresh, errors)` where `account` carries only the long-tail fields
/// and `fresh` marks the ones that succeeded (caller merges via `merge_longtail`).
pub(crate) fn assemble_longtail(
    staking_summary: Result<Value, SyncError>,
    delegations: Result<Value, SyncError>,
    vault_equities: Result<Value, SyncError>,
    borrow_lend: Result<Value, SyncError>,
    agents: Result<Value, SyncError>,
) -> (HlAccount, LongtailFresh, Vec<String>) {
    let mut errors = Vec::new();
    let mut fresh = LongtailFresh::default();
    let mut acct = HlAccount::default();

    match (staking_summary, delegations) {
        (Ok(s), Ok(d)) => match parse_hl_staking_account(&s, &d) {
            Ok(st) => {
                acct.staking = Some(st);
                fresh.staking = true;
            }
            Err(e) => errors.push(format!("staking parse: {e}")),
        },
        (s, d) => {
            if let Err(e) = s {
                errors.push(format!("staking: {e}"));
            }
            if let Err(e) = d {
                errors.push(format!("delegations: {e}"));
            }
        }
    }
    match vault_equities {
        Ok(v) => match parse_hl_vault_equities(&v) {
            Ok(ve) => {
                acct.vault_equities = ve;
                fresh.vault_equities = true;
            }
            Err(e) => errors.push(format!("vault parse: {e}")),
        },
        Err(e) => errors.push(format!("vault: {e}")),
    }
    match borrow_lend {
        Ok(v) => match parse_hl_borrow_lend_account(&v) {
            Ok(bl) => {
                acct.borrow_lend = Some(bl);
                fresh.borrow_lend = true;
            }
            Err(e) => errors.push(format!("borrow_lend parse: {e}")),
        },
        Err(e) => errors.push(format!("borrow_lend: {e}")),
    }
    match agents {
        Ok(v) => match parse_hl_agents(&v) {
            Ok(ag) => {
                acct.agents = ag;
                fresh.agents = true;
            }
            Err(e) => errors.push(format!("agents parse: {e}")),
        },
        Err(e) => errors.push(format!("agents: {e}")),
    }
    (acct, fresh, errors)
}

pub(crate) fn parse_account_snapshot(
    clearinghouse: &Value,
    open_orders: &Value,
    agents: &Value,
    meta: &Value,
) -> Result<HlAccount, SyncError> {
    let symbols = symbol_index(meta);
    let positions = parse_hl_positions(clearinghouse, &symbols)?;
    let open_orders = parse_hl_open_orders(open_orders, &symbols)?;
    let leverage_settings = parse_hl_leverage_settings(clearinghouse, &symbols)?;
    let agents = parse_hl_agents(agents)?;
    let perp_usdc = value_at(clearinghouse, &["withdrawable"])
        .map(state_decimal_from_value)
        .transpose()?;

    Ok(HlAccount {
        perp_usdc,
        perp_dex_margins: Vec::new(),
        pending_outflow: Decimal::new("0"),
        positions,
        open_orders,
        spot_balances: Vec::new(),
        staking: None,
        vault_equities: Vec::new(),
        borrow_lend: None,
        leverage_settings,
        agents,
    })
}

fn merge_hl_account(target: &mut HlAccount, extra: HlAccount) {
    target.perp_usdc = add_optional_decimals(target.perp_usdc.take(), extra.perp_usdc)
        .or_else(|| Some(Decimal::new("0")));
    target.positions.extend(extra.positions);
    target.open_orders.extend(extra.open_orders);
    target.spot_balances.extend(extra.spot_balances);
    if target.staking.is_none() {
        target.staking = extra.staking;
    }
    target.vault_equities.extend(extra.vault_equities);
    if target.borrow_lend.is_none() {
        target.borrow_lend = extra.borrow_lend;
    }
    target.leverage_settings.extend(extra.leverage_settings);
    target.agents.extend(extra.agents);
}

fn add_optional_decimals(lhs: Option<Decimal>, rhs: Option<Decimal>) -> Option<Decimal> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => {
            let lhs = RustDecimal::from_str(lhs.as_str()).ok()?;
            let rhs = RustDecimal::from_str(rhs.as_str()).ok()?;
            Some(Decimal::new((lhs + rhs).normalize().to_string()))
        }
        (Some(lhs), None) | (None, Some(lhs)) => Some(lhs),
        (None, None) => None,
    }
}

fn apply_hl_account_primitives(
    account: &mut HlAccount,
    spot: &Value,
    staking_summary: &Value,
    delegations: &Value,
    vault_equities: &Value,
    borrow_lend: &Value,
) -> Result<(), SyncError> {
    account.spot_balances = parse_hl_spot_balances(spot)?;
    account.staking = Some(parse_hl_staking_account(staking_summary, delegations)?);
    account.vault_equities = parse_hl_vault_equities(vault_equities)?;
    account.borrow_lend = Some(parse_hl_borrow_lend_account(borrow_lend)?);
    Ok(())
}

fn parse_hl_spot_balances(spot: &Value) -> Result<Vec<HlSpotBalance>, SyncError> {
    let mut available = HashMap::new();
    if let Some(items) =
        value_at(spot, &["tokenToAvailableAfterMaintenance"]).and_then(Value::as_array)
    {
        for item in items {
            let pair = item
                .as_array()
                .ok_or_else(|| sync_error("tokenToAvailableAfterMaintenance item not array"))?;
            let token = u32_from_value(
                pair.first()
                    .ok_or_else(|| sync_error("available item missing token"))?,
                "available token",
            )?;
            let amount = state_decimal_from_value(
                pair.get(1)
                    .ok_or_else(|| sync_error("available item missing amount"))?,
            )?;
            available.insert(token, amount);
        }
    }

    let mut out = Vec::new();
    let Some(items) = value_at(spot, &["balances"]).and_then(Value::as_array) else {
        return Ok(out);
    };
    for item in items {
        let token = u32_at(item, &["token"])?;
        out.push(HlSpotBalance {
            coin: string_at(item, &["coin"])?,
            token,
            total: decimal_at(item, &["total"])?,
            hold: decimal_at(item, &["hold"])?,
            entry_ntl: decimal_at(item, &["entryNtl"])?,
            available_after_maintenance: available.remove(&token),
        });
    }
    Ok(out)
}

fn parse_hl_staking_account(
    summary: &Value,
    delegations: &Value,
) -> Result<HlStakingAccount, SyncError> {
    Ok(HlStakingAccount {
        delegated: decimal_at(summary, &["delegated"])?,
        undelegated: decimal_at(summary, &["undelegated"])?,
        total_pending_withdrawal: decimal_at(summary, &["totalPendingWithdrawal"])?,
        n_pending_withdrawals: u32_at(summary, &["nPendingWithdrawals"])?,
        delegations: parse_hl_staking_delegations(delegations)?,
    })
}

fn parse_hl_staking_delegations(
    delegations: &Value,
) -> Result<Vec<HlStakingDelegation>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = delegations.as_array() else {
        return Ok(out);
    };
    for item in items {
        let validator = string_at(item, &["validator"])?;
        out.push(HlStakingDelegation {
            validator: Address::from_str(&validator)
                .map_err(|e| sync_error(format!("validator address {validator}: {e}")))?,
            amount: decimal_at(item, &["amount"])?,
            locked_until_timestamp: value_at(item, &["lockedUntilTimestamp"])
                .filter(|v| !v.is_null())
                .and_then(Value::as_u64),
        });
    }
    Ok(out)
}

fn parse_hl_vault_equities(vault_equities: &Value) -> Result<Vec<HlVaultEquity>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = vault_equities.as_array() else {
        return Ok(out);
    };
    for item in items {
        let vault_address = string_at(item, &["vaultAddress"])?;
        out.push(HlVaultEquity {
            vault_address: Address::from_str(&vault_address)
                .map_err(|e| sync_error(format!("vault address {vault_address}: {e}")))?,
            equity: decimal_at(item, &["equity"])?,
            locked_until_timestamp: value_at(item, &["lockedUntilTimestamp"])
                .filter(|v| !v.is_null())
                .and_then(Value::as_u64),
        });
    }
    Ok(out)
}

fn parse_hl_borrow_lend_account(borrow_lend: &Value) -> Result<HlBorrowLendAccount, SyncError> {
    let mut token_states = Vec::new();
    if let Some(items) = value_at(borrow_lend, &["tokenToState"]).and_then(Value::as_array) {
        for item in items {
            let pair = item
                .as_array()
                .ok_or_else(|| sync_error("tokenToState item not array"))?;
            let token = u32_from_value(
                pair.first()
                    .ok_or_else(|| sync_error("tokenToState item missing token"))?,
                "borrow/lend token",
            )?;
            let state = pair
                .get(1)
                .ok_or_else(|| sync_error("tokenToState item missing state"))?;
            token_states.push(HlBorrowLendTokenState {
                token,
                borrow: parse_hl_borrow_lend_balance(
                    value_at(state, &["borrow"])
                        .ok_or_else(|| sync_error("borrow/lend token missing borrow"))?,
                )?,
                supply: parse_hl_borrow_lend_balance(
                    value_at(state, &["supply"])
                        .ok_or_else(|| sync_error("borrow/lend token missing supply"))?,
                )?,
            });
        }
    }

    Ok(HlBorrowLendAccount {
        token_states,
        health: value_at(borrow_lend, &["health"])
            .and_then(Value::as_str)
            .map(str::to_owned),
        health_factor: value_at(borrow_lend, &["healthFactor"])
            .filter(|v| !v.is_null())
            .map(state_decimal_from_value)
            .transpose()?,
    })
}

fn parse_hl_borrow_lend_balance(balance: &Value) -> Result<HlBorrowLendBalance, SyncError> {
    Ok(HlBorrowLendBalance {
        basis: decimal_at(balance, &["basis"])?,
        value: decimal_at(balance, &["value"])?,
    })
}

fn decimal_at(obj: &Value, path: &[&str]) -> Result<Decimal, SyncError> {
    state_decimal_from_value(
        value_at(obj, path).ok_or_else(|| sync_error(format!("missing {}", path.join("."))))?,
    )
}

fn u32_at(obj: &Value, path: &[&str]) -> Result<u32, SyncError> {
    u32_from_value(
        value_at(obj, path).ok_or_else(|| sync_error(format!("missing {}", path.join("."))))?,
        &path.join("."),
    )
}

fn u32_from_value(value: &Value, label: &str) -> Result<u32, SyncError> {
    let raw = value
        .as_u64()
        .ok_or_else(|| sync_error(format!("{label} is not an unsigned integer")))?;
    u32::try_from(raw).map_err(|e| sync_error(format!("{label} {raw}: {e}")))
}

fn payload_for_parser(
    endpoint: &str,
    parser_id: &str,
    user: &Address,
    market_symbol: Option<&str>,
) -> Result<Value, SyncError> {
    let dex = request_dex(endpoint, market_symbol);
    match parser_id {
        "hl_mids" | "hl_all_mids" => Ok(with_dex(json!({ "type": "allMids" }), dex.as_deref())),
        "hl_oracle" | "hl_funding" | "hl_oi" | "hl_market_meta" => Ok(with_dex(
            json!({ "type": "metaAndAssetCtxs" }),
            dex.as_deref(),
        )),
        "hl_open_orders" => Ok(with_dex(
            json!({ "type": "frontendOpenOrders", "user": hl_user(user) }),
            dex.as_deref(),
        )),
        "hl_account" | "hl_clearinghouse" => Ok(with_dex(
            json!({ "type": "clearinghouseState", "user": hl_user(user) }),
            dex.as_deref(),
        )),
        "hl_spot_account" => Ok(json!({ "type": "spotClearinghouseState", "user": hl_user(user) })),
        "hl_staking_summary" => Ok(json!({ "type": "delegatorSummary", "user": hl_user(user) })),
        "hl_staking_delegations" => Ok(json!({ "type": "delegations", "user": hl_user(user) })),
        "hl_vault_equities" => Ok(json!({ "type": "userVaultEquities", "user": hl_user(user) })),
        "hl_borrow_lend" => Ok(json!({ "type": "borrowLendUserState", "user": hl_user(user) })),
        "hl_fees" => Ok(json!({ "type": "userFees", "user": hl_user(user) })),
        "hl_l2_book" => {
            let Some(symbol) = market_symbol else {
                return Err(sync_error("hl_l2_book requires a market symbol"));
            };
            Ok(json!({ "type": "l2Book", "coin": hl_coin(symbol, dex.as_deref()) }))
        }
        "hl_meta" => Ok(with_dex(json!({ "type": "meta" }), dex.as_deref())),
        "hl_agents" => Ok(json!({ "type": "extraAgents", "user": hl_user(user) })),
        other => Err(SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: format!("unknown parser_id: {other}"),
        }),
    }
}

pub(crate) fn parse_all_mids_value(
    payload: &Value,
    market_symbol: &str,
) -> Result<Value, SyncError> {
    let obj = payload
        .as_object()
        .ok_or_else(|| sync_error("allMids response is not an object"))?;
    for candidate in symbol_candidates(market_symbol) {
        if let Some(v) = obj.get(&candidate) {
            return decimal_string_value(v);
        }
    }
    Err(sync_error(format!(
        "allMids missing market {market_symbol}"
    )))
}

pub(crate) fn parse_asset_ctx_value(
    payload: &Value,
    market_symbol: &str,
    metric: HlAssetMetric,
) -> Result<Value, SyncError> {
    let arr = payload
        .as_array()
        .ok_or_else(|| sync_error("metaAndAssetCtxs response is not an array"))?;
    let meta = arr
        .first()
        .ok_or_else(|| sync_error("metaAndAssetCtxs missing meta"))?;
    let ctxs = arr
        .get(1)
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("metaAndAssetCtxs missing asset contexts"))?;
    let index = symbol_index(meta)
        .into_iter()
        .find_map(|(sym, ix)| symbol_matches(&sym, market_symbol).then_some(ix))
        .ok_or_else(|| sync_error(format!("meta missing market {market_symbol}")))?;
    let ctx = ctxs
        .get(usize::try_from(index).map_err(|e| sync_error(format!("asset index: {e}")))?)
        .ok_or_else(|| sync_error(format!("asset context missing index {index}")))?;

    match metric {
        HlAssetMetric::MarkPrice => decimal_string_from_key(ctx, "markPx"),
        HlAssetMetric::OraclePrice => decimal_string_from_key(ctx, "oraclePx"),
        HlAssetMetric::Funding => decimal_string_from_key(ctx, "funding"),
        HlAssetMetric::OpenInterest => decimal_integer_string_from_key(ctx, "openInterest"),
        HlAssetMetric::MaxLeverage => {
            let market = universe_item(meta, market_symbol)?;
            decimal_string_from_key(market, "maxLeverage")
        }
        HlAssetMetric::InitialMarginBp => {
            let market = universe_item(meta, market_symbol)?;
            let max_lev = decimal_from_value(
                value_at(market, &["maxLeverage"])
                    .ok_or_else(|| sync_error("missing maxLeverage"))?,
            )?;
            let bp = (RustDecimal::from(10_000_u32) / max_lev)
                .ceil()
                .to_u64()
                .ok_or_else(|| sync_error("initial margin bp out of range"))?;
            Ok(Value::from(bp))
        }
        HlAssetMetric::MaintenanceMarginBp => {
            let market = universe_item(meta, market_symbol)?;
            let max_lev = decimal_from_value(
                value_at(market, &["maxLeverage"])
                    .ok_or_else(|| sync_error("missing maxLeverage"))?,
            )?;
            let initial = (RustDecimal::from(10_000_u32) / max_lev).ceil();
            let bp = (initial / RustDecimal::from(2_u32))
                .ceil()
                .to_u64()
                .ok_or_else(|| sync_error("maintenance margin bp out of range"))?;
            Ok(Value::from(bp))
        }
    }
}

pub(crate) fn parse_user_fee_bp(payload: &Value, side: FeeSide) -> Result<Value, SyncError> {
    let key = match side {
        FeeSide::Maker => "userAddRate",
        FeeSide::Taker => "userCrossRate",
    };
    let rate = decimal_from_value(
        value_at(payload, &[key]).ok_or_else(|| sync_error(format!("userFees missing {key}")))?,
    )?;
    let bp = (rate * RustDecimal::from(10_000_u32))
        .round()
        .to_u64()
        .ok_or_else(|| sync_error("fee bp out of range"))?;
    Ok(Value::from(bp))
}

pub(crate) fn parse_live_input_value(
    parser_id: &str,
    slot: &ActionSlot,
    market_symbol: &str,
    payload: &Value,
) -> Result<Value, SyncError> {
    match (parser_id, slot) {
        (
            "hl_mids" | "hl_all_mids",
            ActionSlot::PerpOpenMarkPrice
            | ActionSlot::PerpCloseMarkPrice
            | ActionSlot::PerpIncreaseMarkPrice
            | ActionSlot::PerpDecreaseMarkPrice
            | ActionSlot::PerpPlaceLimitMarkPrice
            | ActionSlot::PerpPlaceStopMarkPrice,
        ) => parse_all_mids_value(payload, market_symbol),

        ("hl_oracle", ActionSlot::PerpOpenOraclePrice | ActionSlot::PerpIncreaseOraclePrice) => {
            parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::OraclePrice)
        }

        ("hl_funding", ActionSlot::PerpOpenFundingRate | ActionSlot::PerpIncreaseFundingRate) => {
            parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::Funding)
        }

        ("hl_oi", ActionSlot::PerpOpenAvailableOi | ActionSlot::PerpIncreaseAvailableOi) => {
            parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::OpenInterest)
        }

        (
            "hl_market_meta",
            ActionSlot::PerpOpenMaxLeverage
            | ActionSlot::PerpIncreaseMaxLeverage
            | ActionSlot::PerpChangeLeverageMaxLeverage,
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::MaxLeverage),

        (
            "hl_market_meta",
            ActionSlot::PerpOpenInitialMarginBp | ActionSlot::PerpIncreaseInitialMarginBp,
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::InitialMarginBp),

        (
            "hl_market_meta",
            ActionSlot::PerpOpenMaintenanceBp | ActionSlot::PerpIncreaseMaintenanceBp,
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::MaintenanceMarginBp),

        ("hl_fees", ActionSlot::PerpOpenFeeMakerBp | ActionSlot::PerpIncreaseFeeMakerBp) => {
            parse_user_fee_bp(payload, FeeSide::Maker)
        }

        (
            "hl_fees",
            ActionSlot::PerpOpenFeeTakerBp
            | ActionSlot::PerpIncreaseFeeTakerBp
            | ActionSlot::PerpCloseFeeBp
            | ActionSlot::PerpDecreaseFeeBp,
        ) => parse_user_fee_bp(payload, FeeSide::Taker),

        (
            "hl_account",
            ActionSlot::PerpOpenUserAccountState
            | ActionSlot::PerpIncreaseUserAccountState
            | ActionSlot::PerpPlaceLimitUserAccountState
            | ActionSlot::PerpPlaceStopUserAccountState,
        ) => serde_json::to_value(parse_perp_account_state(payload)?)
            .map_err(|e| sync_error(format!("serialize account state: {e}"))),

        ("hl_open_orders", ActionSlot::PerpPlaceLimitOpenOrdersCount) => payload
            .as_array()
            .map(|orders| Value::from(orders.len() as u64))
            .ok_or_else(|| sync_error("openOrders response is not an array")),

        ("hl_l2_book", ActionSlot::PerpPlaceLimitBestBidAsk) => parse_l2_best_bid_ask(payload),

        _ => Err(sync_error(format!(
            "unsupported Hyperliquid parser/slot: {parser_id}/{slot:?}"
        ))),
    }
}

pub(crate) fn parse_state_value(
    parser_id: &str,
    location: &FieldLocation,
    market_symbol: &str,
    payload: &Value,
) -> Result<Value, SyncError> {
    match (parser_id, location) {
        ("hl_mids" | "hl_all_mids", FieldLocation::PerpMarkPrice { .. }) => {
            parse_all_mids_value(payload, market_symbol)
        }
        (
            "hl_oracle" | "hl_funding" | "hl_oi" | "hl_market_meta",
            FieldLocation::PerpMarkPrice { .. },
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::MarkPrice),
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpLiqPrice { .. }) => {
            parse_clearinghouse_position_value(
                payload,
                market_symbol,
                HlPositionMetric::LiquidationPrice,
            )
        }
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpUnrealizedPnl { .. }) => {
            parse_clearinghouse_position_value(
                payload,
                market_symbol,
                HlPositionMetric::UnrealizedPnl,
            )
        }
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpFundingOwed { .. }) => {
            parse_clearinghouse_position_value(
                payload,
                market_symbol,
                HlPositionMetric::FundingOwed,
            )
        }
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpLeverage { .. }) => {
            parse_clearinghouse_position_value(payload, market_symbol, HlPositionMetric::Leverage)
        }
        _ => Err(sync_error(format!(
            "unsupported Hyperliquid parser/state location: {parser_id}/{location:?}"
        ))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HlPositionMetric {
    LiquidationPrice,
    UnrealizedPnl,
    FundingOwed,
    Leverage,
}

fn parse_clearinghouse_position_value(
    clearinghouse: &Value,
    market_symbol: &str,
    metric: HlPositionMetric,
) -> Result<Value, SyncError> {
    let position = clearinghouse_position(clearinghouse, market_symbol)?;
    match metric {
        HlPositionMetric::LiquidationPrice => value_at(position, &["liquidationPx"])
            .filter(|v| !v.is_null())
            .map(decimal_string_value)
            .transpose()
            .map(|v| v.unwrap_or(Value::Null)),
        HlPositionMetric::UnrealizedPnl => {
            decimal_integer_string_from_key(position, "unrealizedPnl")
        }
        HlPositionMetric::FundingOwed => decimal_integer_string_from_key(
            value_at(position, &["cumFunding"])
                .ok_or_else(|| sync_error("position missing cumFunding"))?,
            "sinceOpen",
        ),
        HlPositionMetric::Leverage => decimal_string_from_key(
            value_at(position, &["leverage"])
                .ok_or_else(|| sync_error("position missing leverage"))?,
            "value",
        ),
    }
}

fn parse_perp_account_state(clearinghouse: &Value) -> Result<PerpAccountState, SyncError> {
    let total_collateral_usd = decimal_u256_at(clearinghouse, &["marginSummary", "accountValue"])?;
    let used_margin_usd = decimal_u256_at(clearinghouse, &["marginSummary", "totalMarginUsed"])?;
    let free_margin_usd = decimal_u256_at(clearinghouse, &["withdrawable"])?;
    let mut open_positions = Vec::new();

    if let Some(items) = value_at(clearinghouse, &["assetPositions"]).and_then(Value::as_array) {
        for item in items {
            let position = value_at(item, &["position"])
                .ok_or_else(|| sync_error("assetPosition missing position"))?;
            let symbol = string_at(position, &["coin"])?;
            let szi = decimal_from_value(
                value_at(position, &["szi"]).ok_or_else(|| sync_error("position missing szi"))?,
            )?;
            if szi.is_zero() {
                continue;
            }
            let exposure = value_at(position, &["positionValue"])
                .map(decimal_from_value)
                .transpose()?
                .unwrap_or_else(|| szi.abs());
            open_positions.push((
                MarketRef {
                    symbol,
                    venue: VenueRef::new("hyperliquid"),
                },
                decimal_to_u256(exposure)?,
            ));
        }
    }

    Ok(PerpAccountState {
        total_collateral_usd,
        used_margin_usd,
        free_margin_usd,
        open_positions,
    })
}

fn parse_l2_best_bid_ask(payload: &Value) -> Result<Value, SyncError> {
    let levels = value_at(payload, &["levels"])
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("l2Book response missing levels"))?;
    let bid = levels
        .first()
        .and_then(Value::as_array)
        .and_then(|side| side.first())
        .and_then(|level| value_at(level, &["px"]))
        .ok_or_else(|| sync_error("l2Book missing best bid"))?;
    let ask = levels
        .get(1)
        .and_then(Value::as_array)
        .and_then(|side| side.first())
        .and_then(|level| value_at(level, &["px"]))
        .ok_or_else(|| sync_error("l2Book missing best ask"))?;

    Ok(json!([
        decimal_string_value(bid)?,
        decimal_string_value(ask)?
    ]))
}

fn parse_hl_positions(
    clearinghouse: &Value,
    symbols: &HashMap<String, u32>,
) -> Result<Vec<HlPosition>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = value_at(clearinghouse, &["assetPositions"]).and_then(Value::as_array) else {
        return Ok(out);
    };
    for item in items {
        let position = value_at(item, &["position"])
            .ok_or_else(|| sync_error("assetPosition missing position"))?;
        let symbol = string_at(position, &["coin"])?;
        let szi = decimal_from_value(
            value_at(position, &["szi"]).ok_or_else(|| sync_error("position missing szi"))?,
        )?;
        if szi.is_zero() {
            continue;
        }
        let entry_price = value_at(position, &["entryPx"])
            .filter(|v| !v.is_null())
            .map(state_decimal_from_value)
            .transpose()?
            .unwrap_or_else(|| Decimal::new("0"));
        let asset_index = symbol_to_index(symbols, &symbol)?;
        out.push(HlPosition {
            asset_index,
            symbol: Some(symbol),
            is_long: szi.is_sign_positive(),
            size: Decimal::new(szi.abs().normalize().to_string()),
            entry_price,
            dex: None,
            liquidation_price: None,
        });
    }
    Ok(out)
}

fn parse_hl_open_orders(
    open_orders: &Value,
    symbols: &HashMap<String, u32>,
) -> Result<Vec<HlOpenOrder>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = open_orders.as_array() else {
        return Ok(out);
    };
    for item in items {
        let symbol = string_at(item, &["coin"])?;
        out.push(HlOpenOrder {
            asset_index: symbol_to_index(symbols, &symbol)?,
            symbol: Some(symbol),
            is_buy: parse_side_is_buy(string_at(item, &["side"])?)?,
            price: state_decimal_from_value(
                value_at(item, &["limitPx"]).ok_or_else(|| sync_error("order missing limitPx"))?,
            )?,
            size: state_decimal_from_value(
                value_at(item, &["sz"]).ok_or_else(|| sync_error("order missing sz"))?,
            )?,
            reduce_only: value_at(item, &["reduceOnly"])
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tif: normalize_tif(value_at(item, &["tif"]).and_then(Value::as_str)),
            oid: value_at(item, &["oid"]).and_then(Value::as_u64),
            order_type: value_at(item, &["orderType"])
                .and_then(Value::as_str)
                .map(str::to_owned),
            is_trigger: value_at(item, &["isTrigger"]).and_then(Value::as_bool),
            trigger_price: value_at(item, &["triggerPx"])
                .filter(|v| !v.is_null())
                .map(state_decimal_from_value)
                .transpose()?,
            trigger_condition: value_at(item, &["triggerCondition"])
                .and_then(Value::as_str)
                .map(str::to_owned),
            is_position_tpsl: value_at(item, &["isPositionTpsl"]).and_then(Value::as_bool),
            dex: None,
        });
    }
    Ok(out)
}

fn parse_hl_leverage_settings(
    clearinghouse: &Value,
    symbols: &HashMap<String, u32>,
) -> Result<Vec<HlLeverageSetting>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = value_at(clearinghouse, &["assetPositions"]).and_then(Value::as_array) else {
        return Ok(out);
    };
    for item in items {
        let position = value_at(item, &["position"])
            .ok_or_else(|| sync_error("assetPosition missing position"))?;
        let symbol = string_at(position, &["coin"])?;
        let leverage = value_at(position, &["leverage"])
            .ok_or_else(|| sync_error("position missing leverage"))?;
        let value = value_at(leverage, &["value"])
            .and_then(Value::as_u64)
            .ok_or_else(|| sync_error("position leverage value missing"))?;
        out.push(HlLeverageSetting {
            asset_index: symbol_to_index(symbols, &symbol)?,
            is_cross: matches!(
                value_at(leverage, &["type"]).and_then(Value::as_str),
                Some("cross" | "Cross")
            ),
            leverage: u32::try_from(value).map_err(|e| sync_error(format!("leverage: {e}")))?,
            dex: None,
        });
    }
    Ok(out)
}

fn parse_hl_agents(agents: &Value) -> Result<Vec<HlAgentApproval>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = agents.as_array() else {
        return Ok(out);
    };
    for item in items {
        let address = string_at(item, &["address"])?;
        out.push(HlAgentApproval {
            agent_address: Address::from_str(&address)
                .map_err(|e| sync_error(format!("agent address {address}: {e}")))?,
            agent_name: value_at(item, &["name"])
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        });
    }
    Ok(out)
}

fn clearinghouse_position<'a>(
    clearinghouse: &'a Value,
    market_symbol: &str,
) -> Result<&'a Value, SyncError> {
    let items = value_at(clearinghouse, &["assetPositions"])
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("clearinghouseState missing assetPositions"))?;
    items
        .iter()
        .filter_map(|item| value_at(item, &["position"]))
        .find(|position| {
            value_at(position, &["coin"])
                .and_then(Value::as_str)
                .is_some_and(|coin| symbol_matches(coin, market_symbol))
        })
        .ok_or_else(|| sync_error(format!("clearinghouseState missing market {market_symbol}")))
}

fn venue_source_parts(source: &DataSource) -> Result<(&str, &str), SyncError> {
    match source {
        DataSource::VenueApi {
            endpoint,
            parser_id,
            ..
        } => Ok((endpoint.as_str(), parser_id.as_str())),
        _ => Err(SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: "not a VenueApi source".into(),
        }),
    }
}

fn hl_user(user: &Address) -> String {
    format!("{user:#x}")
}

fn hl_coin(market_symbol: &str, dex: Option<&str>) -> String {
    let coin = strip_quote_suffix(market_symbol);
    if coin.contains(':') {
        return coin.to_owned();
    }
    match dex {
        Some(dex) if !dex.is_empty() => format!("{dex}:{coin}"),
        _ => coin.to_owned(),
    }
}

fn request_dex(endpoint: &str, market_symbol: Option<&str>) -> Option<String> {
    market_symbol
        .and_then(market_dex)
        .map(str::to_owned)
        .or_else(|| endpoint_dex(endpoint))
}

fn market_dex(market_symbol: &str) -> Option<&str> {
    let (dex, _coin) = market_symbol.split_once(':')?;
    (!dex.is_empty()).then_some(dex)
}

fn endpoint_dex(endpoint: &str) -> Option<String> {
    let query = endpoint.split_once('?')?.1;
    query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "dex" && !value.is_empty()).then(|| value.to_owned())
    })
}

fn with_dex(mut body: Value, dex: Option<&str>) -> Value {
    if let Some(dex) = dex.filter(|dex| !dex.is_empty()) {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("dex".to_owned(), Value::String(dex.to_owned()));
        }
    }
    body
}

fn symbol_index(meta: &Value) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    if let Some(universe) = value_at(meta, &["universe"]).and_then(Value::as_array) {
        for (i, item) in universe.iter().enumerate() {
            if let Some(name) = value_at(item, &["name"]).and_then(Value::as_str) {
                if let Ok(ix) = u32::try_from(i) {
                    out.insert(name.to_owned(), ix);
                }
            }
        }
    }
    out
}

fn universe_item<'a>(meta: &'a Value, market_symbol: &str) -> Result<&'a Value, SyncError> {
    let universe = value_at(meta, &["universe"])
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("meta missing universe"))?;
    universe
        .iter()
        .find(|item| {
            value_at(item, &["name"])
                .and_then(Value::as_str)
                .is_some_and(|name| symbol_matches(name, market_symbol))
        })
        .ok_or_else(|| sync_error(format!("meta missing market {market_symbol}")))
}

fn symbol_to_index(symbols: &HashMap<String, u32>, symbol: &str) -> Result<u32, SyncError> {
    for (known, ix) in symbols {
        if symbol_matches(known, symbol) {
            return Ok(*ix);
        }
    }
    Err(sync_error(format!("unknown Hyperliquid symbol {symbol}")))
}

fn symbol_candidates(symbol: &str) -> Vec<String> {
    let mut out = vec![symbol.to_owned(), strip_quote_suffix(symbol).to_owned()];
    if let Some((_dex, base)) = strip_quote_suffix(symbol).split_once(':') {
        out.push(base.to_owned());
    }
    out.sort();
    out.dedup();
    out
}

fn symbol_matches(lhs: &str, rhs: &str) -> bool {
    let lhs = symbol_candidates(lhs);
    let rhs = symbol_candidates(rhs);
    lhs.iter().any(|l| rhs.iter().any(|r| l == r))
}

fn strip_quote_suffix(symbol: &str) -> &str {
    for suffix in ["-USD", "-USDC", "-PERP", "/USD", "/USDC"] {
        if let Some(stripped) = symbol.strip_suffix(suffix) {
            return stripped;
        }
    }
    symbol
}

fn parse_side_is_buy(side: String) -> Result<bool, SyncError> {
    match side.as_str() {
        "B" | "b" | "bid" | "buy" | "long" | "Long" => Ok(true),
        "A" | "a" | "ask" | "sell" | "short" | "Short" => Ok(false),
        other => Err(sync_error(format!("unknown Hyperliquid side {other}"))),
    }
}

fn normalize_tif(tif: Option<&str>) -> String {
    match tif.unwrap_or("Gtc") {
        "Gtc" | "gtc" => "gtc",
        "Ioc" | "ioc" => "ioc",
        "Alo" | "alo" | "PostOnly" | "post_only" => "post_only",
        "Fok" | "fok" => "fok",
        other => other,
    }
    .to_owned()
}

fn decimal_string_from_key(obj: &Value, key: &str) -> Result<Value, SyncError> {
    decimal_string_value(value_at(obj, &[key]).ok_or_else(|| sync_error(format!("missing {key}")))?)
}

fn decimal_integer_string_from_key(obj: &Value, key: &str) -> Result<Value, SyncError> {
    let d = decimal_from_value(
        value_at(obj, &[key]).ok_or_else(|| sync_error(format!("missing {key}")))?,
    )?;
    Ok(Value::String(d.trunc().normalize().to_string()))
}

fn decimal_string_value(value: &Value) -> Result<Value, SyncError> {
    Ok(Value::String(state_decimal_from_value(value)?.0))
}

fn state_decimal_from_value(value: &Value) -> Result<Decimal, SyncError> {
    let d = decimal_from_value(value)?;
    Ok(Decimal::new(d.normalize().to_string()))
}

fn decimal_u256_at(obj: &Value, path: &[&str]) -> Result<U256, SyncError> {
    let value = value_at(obj, path)
        .ok_or_else(|| sync_error(format!("missing decimal {}", path.join("."))))?;
    decimal_to_u256(decimal_from_value(value)?)
}

fn decimal_to_u256(value: RustDecimal) -> Result<U256, SyncError> {
    if value.is_sign_negative() {
        return Err(sync_error(format!("negative unsigned decimal {value}")));
    }
    let s = value.trunc().normalize().to_string();
    U256::from_str_radix(&s, 10).map_err(|e| sync_error(format!("u256 {s}: {e}")))
}

fn decimal_from_value(value: &Value) -> Result<RustDecimal, SyncError> {
    match value {
        Value::String(s) => {
            RustDecimal::from_str(s).map_err(|e| sync_error(format!("decimal {s:?}: {e}")))
        }
        Value::Number(n) => RustDecimal::from_str(&n.to_string())
            .map_err(|e| sync_error(format!("decimal {n}: {e}"))),
        other => Err(sync_error(format!("expected decimal, got {other}"))),
    }
}

fn string_at(obj: &Value, path: &[&str]) -> Result<String, SyncError> {
    value_at(obj, path)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| sync_error(format!("missing string {}", path.join("."))))
}

fn value_at<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}

fn sync_error(reason: impl Into<String>) -> SyncError {
    SyncError::FetchFailed {
        source_id: "hyperliquid".into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::DataSource;
    use policy_state::Decimal;

    #[test]
    fn assemble_core_is_best_effort() {
        // clearinghouse OK, spot FAILS, openOrders OK, meta OK.
        let clearinghouse = serde_json::json!({ "withdrawable": "600.5", "assetPositions": [] });
        let open_orders = serde_json::json!([]);
        let meta = serde_json::json!({ "universe": [] });
        let spot_err: Result<Value, SyncError> = Err(SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: "boom".into(),
        });

        let (acct, fresh, errors) =
            assemble_core(Ok(clearinghouse), spot_err, Ok(open_orders), &meta);
        // core present despite spot failure
        assert_eq!(acct.perp_usdc, Some(Decimal::new("600.5")));
        assert!(acct.spot_balances.is_empty()); // failed → left empty
        assert!(fresh.fresh_dexs.contains(&None)); // native dex bundle fresh
        assert!(!fresh.spot); // spot NOT fresh → caller preserves prior value
        assert_eq!(errors.len(), 1); // one recorded error (spot)
        assert!(errors[0].contains("spot"));
    }

    #[test]
    fn assemble_longtail_is_best_effort() {
        // vaults OK (empty), agents FAIL → agents unmarked + one recorded error.
        let staking = serde_json::json!({
            "delegated": "0",
            "undelegated": "0",
            "totalPendingWithdrawal": "0",
            "nPendingWithdrawals": 0
        });
        let delegations = serde_json::json!([]);
        let vaults = serde_json::json!([]);
        let borrow = serde_json::json!({});
        let agents_err: Result<Value, SyncError> = Err(SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: "boom".into(),
        });

        let (lt, fresh, errors) = assemble_longtail(
            Ok(staking),
            Ok(delegations),
            Ok(vaults),
            Ok(borrow),
            agents_err,
        );
        assert!(lt.agents.is_empty()); // failed → left empty
        assert!(!fresh.agents); // NOT fresh → caller preserves prior agents
        assert!(fresh.vault_equities); // vaults parsed OK → fresh
        assert!(errors.iter().any(|e| e.contains("agents")));
    }

    /// Live HL fetch for a real address. Set `HL_LIVE_ADDR` and run with
    /// `--ignored --nocapture` to print the parsed core + long-tail snapshot.
    #[tokio::test]
    #[ignore = "hits the live Hyperliquid API; set HL_LIVE_ADDR"]
    async fn fetch_hl_live() {
        let Ok(addr_str) = std::env::var("HL_LIVE_ADDR") else {
            eprintln!("HL_LIVE_ADDR not set — skipping");
            return;
        };
        let user = Address::from_str(addr_str.trim()).expect("valid 0x address");
        let f = HyperliquidFetcher::new();
        let now = Time::from_unix(0);

        let (core, fresh, errors) = f.fetch_hl_core("", &user, now).await;
        println!(
            "=== CORE  fresh{{dexs:{:?} spot:{}}} ===",
            fresh.fresh_dexs, fresh.spot
        );
        println!("perp_usdc (margin): {:?}", core.perp_usdc);
        println!("positions ({}):", core.positions.len());
        for p in &core.positions {
            println!("  {p:?}");
        }
        println!("spot_balances ({}):", core.spot_balances.len());
        for b in &core.spot_balances {
            println!("  {b:?}");
        }
        println!("open_orders ({}):", core.open_orders.len());
        for o in &core.open_orders {
            println!("  {o:?}");
        }
        println!("leverage_settings: {}", core.leverage_settings.len());
        println!("core errors: {errors:?}");

        let (lt, lfresh, lerrors) = f.fetch_hl_longtail("", &user).await;
        println!(
            "=== LONG-TAIL  fresh{{staking:{} vault:{} borrow_lend:{} agents:{}}} ===",
            lfresh.staking, lfresh.vault_equities, lfresh.borrow_lend, lfresh.agents
        );
        println!("staking: {:?}", lt.staking);
        println!(
            "vault_equities ({}): {:?}",
            lt.vault_equities.len(),
            lt.vault_equities
        );
        println!("borrow_lend: {:?}", lt.borrow_lend);
        println!("agents ({}): {:?}", lt.agents.len(), lt.agents);
        println!("long-tail errors: {lerrors:?}");
    }

    #[test]
    fn rejects_non_venue_source() {
        let f = HyperliquidFetcher::new();
        let bad = DataSource::UserSupplied;
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(f.fetch(&bad));
        assert!(res.is_err());
    }

    #[test]
    fn rejects_unknown_parser() {
        let f = HyperliquidFetcher::new();
        let bad = DataSource::VenueApi {
            endpoint: HL_API_BASE.into(),
            parser_id: "made_up".into(),
            auth: None,
        };
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(f.fetch(&bad));
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("unknown parser_id"));
    }

    #[test]
    fn parses_account_snapshot_from_hypersdk_shapes() {
        let clearinghouse = json!({
            "marginSummary": {
                "accountValue": "1000.5",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000.5",
                "totalMarginUsed": "1200"
            },
            "crossMarginSummary": {
                "accountValue": "1000.5",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000.5",
                "totalMarginUsed": "1200"
            },
            "crossMaintenanceMarginUsed": "50",
            "withdrawable": "600.5",
            "assetPositions": [{
                "type": "oneWay",
                "position": {
                    "coin": "BTC",
                    "szi": "0.1",
                    "leverage": { "type": "cross", "value": 5 },
                    "entryPx": "60000",
                    "positionValue": "6000",
                    "unrealizedPnl": "12.3",
                    "returnOnEquity": "0.02",
                    "liquidationPx": "50000",
                    "marginUsed": "1200",
                    "maxLeverage": 50,
                    "cumFunding": {
                        "allTime": "-1",
                        "sinceOpen": "-1",
                        "sinceChange": "0"
                    }
                }
            }],
            "time": 1_710_000_000_123_u64
        });
        let open_orders = json!([{
            "timestamp": 1_710_000_000_124_u64,
            "coin": "ETH",
            "side": "A",
            "limitPx": "3000",
            "sz": "0.25",
            "oid": 42,
            "origSz": "0.25",
            "cloid": null,
            "orderType": "Limit",
            "tif": "Ioc",
            "reduceOnly": true
        }]);
        let agents = json!([{
            "name": "bot",
            "address": "0x1111111111111111111111111111111111111111",
            "validUntil": 1_710_000_000_999_u64
        }]);
        let meta = json!({
            "universe": [
                { "name": "BTC", "maxLeverage": 50, "szDecimals": 5 },
                { "name": "ETH", "maxLeverage": 25, "szDecimals": 4 }
            ],
            "collateralToken": 0
        });

        let acct = parse_account_snapshot(&clearinghouse, &open_orders, &agents, &meta).unwrap();

        assert_eq!(acct.perp_usdc, Some(Decimal::new("600.5")));
        assert_eq!(acct.pending_outflow, Decimal::new("0"));
        assert_eq!(acct.positions.len(), 1);
        assert_eq!(acct.positions[0].asset_index, 0);
        assert_eq!(acct.positions[0].symbol.as_deref(), Some("BTC"));
        assert!(acct.positions[0].is_long);
        assert_eq!(acct.positions[0].size, Decimal::new("0.1"));
        assert_eq!(acct.positions[0].entry_price, Decimal::new("60000"));
        assert_eq!(acct.open_orders.len(), 1);
        assert_eq!(acct.open_orders[0].asset_index, 1);
        assert_eq!(acct.open_orders[0].symbol.as_deref(), Some("ETH"));
        assert!(!acct.open_orders[0].is_buy);
        assert_eq!(acct.open_orders[0].tif, "ioc");
        assert_eq!(acct.open_orders[0].oid, Some(42));
        assert_eq!(acct.leverage_settings.len(), 1);
        assert_eq!(acct.leverage_settings[0].asset_index, 0);
        assert!(acct.leverage_settings[0].is_cross);
        assert_eq!(acct.leverage_settings[0].leverage, 5);
        assert_eq!(acct.agents.len(), 1);
        assert_eq!(acct.agents[0].agent_name.as_deref(), Some("bot"));
    }

    #[test]
    fn parses_non_aggregate_account_primitives_from_hyperliquid_payloads() {
        let mut acct = HlAccount::default();
        let spot = json!({
            "balances": [
                {
                    "coin": "USDC",
                    "token": 0,
                    "total": "1125.961894",
                    "hold": "1077.497057",
                    "entryNtl": "0.0"
                },
                {
                    "coin": "USDT0",
                    "token": 268,
                    "total": "0.446687",
                    "hold": "0.0",
                    "entryNtl": "0.446687"
                }
            ],
            "tokenToAvailableAfterMaintenance": [[0, "48.464837"], [268, "0.446687"]]
        });
        let staking_summary = json!({
            "delegated": "0.0",
            "undelegated": "0.0",
            "totalPendingWithdrawal": "46.84529183",
            "nPendingWithdrawals": 1
        });
        let delegations = json!([{
            "validator": "0x2222222222222222222222222222222222222222",
            "amount": "47.0",
            "lockedUntilTimestamp": 1_735_466_781_353_u64
        }]);
        let vaults = json!([{
            "vaultAddress": "0x3333333333333333333333333333333333333333",
            "equity": "742500.082809",
            "lockedUntilTimestamp": 1_741_132_800_000_u64
        }]);
        let borrow_lend = json!({
            "tokenToState": [[
                0,
                {
                    "borrow": { "basis": "0.0", "value": "0.0" },
                    "supply": {
                        "basis": "44.69295862",
                        "value": "44.69692314"
                    }
                }
            ]],
            "health": "healthy",
            "healthFactor": null
        });

        apply_hl_account_primitives(
            &mut acct,
            &spot,
            &staking_summary,
            &delegations,
            &vaults,
            &borrow_lend,
        )
        .unwrap();

        assert_eq!(acct.spot_balances.len(), 2);
        assert_eq!(acct.spot_balances[0].coin, "USDC");
        assert_eq!(acct.spot_balances[0].token, 0);
        assert_eq!(acct.spot_balances[0].total, Decimal::new("1125.961894"));
        assert_eq!(acct.spot_balances[0].hold, Decimal::new("1077.497057"));
        assert_eq!(acct.spot_balances[0].entry_ntl, Decimal::new("0"));
        assert_eq!(
            acct.spot_balances[0].available_after_maintenance,
            Some(Decimal::new("48.464837"))
        );
        assert_eq!(acct.spot_balances[1].coin, "USDT0");
        assert_eq!(
            acct.spot_balances[1].available_after_maintenance,
            Some(Decimal::new("0.446687"))
        );

        let staking = acct.staking.as_ref().unwrap();
        assert_eq!(
            staking.total_pending_withdrawal,
            Decimal::new("46.84529183")
        );
        assert_eq!(staking.n_pending_withdrawals, 1);
        assert_eq!(staking.delegations.len(), 1);
        assert_eq!(staking.delegations[0].amount, Decimal::new("47"));

        assert_eq!(acct.vault_equities.len(), 1);
        assert_eq!(acct.vault_equities[0].equity, Decimal::new("742500.082809"));
        assert_eq!(
            acct.vault_equities[0].locked_until_timestamp,
            Some(1_741_132_800_000_u64)
        );

        let borrow_lend = acct.borrow_lend.as_ref().unwrap();
        assert_eq!(borrow_lend.health.as_deref(), Some("healthy"));
        assert_eq!(borrow_lend.token_states.len(), 1);
        assert_eq!(
            borrow_lend.token_states[0].supply.value,
            Decimal::new("44.69692314")
        );
    }

    #[test]
    fn extracts_hyperliquid_live_values_from_info_payloads() {
        let mids = json!({ "BTC": "60001" });
        assert_eq!(parse_all_mids_value(&mids, "BTC").unwrap(), json!("60001"));

        let meta_and_ctx = json!([
            {
                "universe": [
                    { "name": "BTC", "maxLeverage": 50, "szDecimals": 5 }
                ],
                "collateralToken": 0
            },
            [{
                "funding": "0.0001",
                "openInterest": "12345.7",
                "markPx": "60000",
                "oraclePx": "59990",
                "midPx": "60001",
                "premium": "0",
                "prevDayPx": "59000",
                "dayNtlVlm": "1000000",
                "impactPxs": ["59999", "60002"]
            }]
        ]);
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::MarkPrice).unwrap(),
            json!("60000")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::OraclePrice).unwrap(),
            json!("59990")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::Funding).unwrap(),
            json!("0.0001")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::OpenInterest).unwrap(),
            json!("12345")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::MaxLeverage).unwrap(),
            json!("50")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::InitialMarginBp).unwrap(),
            json!(200u64)
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::MaintenanceMarginBp)
                .unwrap(),
            json!(100u64)
        );

        let fees = json!({
            "userAddRate": "0.0001",
            "userCrossRate": "0.0005",
            "activeReferralDiscount": "0"
        });
        assert_eq!(
            parse_user_fee_bp(&fees, FeeSide::Maker).unwrap(),
            json!(1u64)
        );
        assert_eq!(
            parse_user_fee_bp(&fees, FeeSide::Taker).unwrap(),
            json!(5u64)
        );
    }

    #[test]
    fn builds_dex_aware_payloads_from_prefixed_markets() {
        let user = Address::from([0x22; 20]);
        let endpoint = "https://api.hyperliquid.xyz/info";

        assert_eq!(
            payload_for_parser(endpoint, "hl_account", &user, Some("xyz:SPCX")).unwrap(),
            json!({
                "type": "clearinghouseState",
                "user": "0x2222222222222222222222222222222222222222",
                "dex": "xyz"
            })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_open_orders", &user, Some("xyz:SPCX")).unwrap(),
            json!({
                "type": "frontendOpenOrders",
                "user": "0x2222222222222222222222222222222222222222",
                "dex": "xyz"
            })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_mids", &user, Some("xyz:SPCX")).unwrap(),
            json!({ "type": "allMids", "dex": "xyz" })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_funding", &user, Some("xyz:SPCX")).unwrap(),
            json!({ "type": "metaAndAssetCtxs", "dex": "xyz" })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_l2_book", &user, Some("xyz:SPCX")).unwrap(),
            json!({ "type": "l2Book", "coin": "xyz:SPCX" })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_spot_account", &user, Some("xyz:SPCX")).unwrap(),
            json!({
                "type": "spotClearinghouseState",
                "user": "0x2222222222222222222222222222222222222222"
            })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_staking_summary", &user, Some("xyz:SPCX")).unwrap(),
            json!({
                "type": "delegatorSummary",
                "user": "0x2222222222222222222222222222222222222222"
            })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_staking_delegations", &user, Some("xyz:SPCX"))
                .unwrap(),
            json!({
                "type": "delegations",
                "user": "0x2222222222222222222222222222222222222222"
            })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_vault_equities", &user, Some("xyz:SPCX")).unwrap(),
            json!({
                "type": "userVaultEquities",
                "user": "0x2222222222222222222222222222222222222222"
            })
        );
        assert_eq!(
            payload_for_parser(endpoint, "hl_borrow_lend", &user, Some("xyz:SPCX")).unwrap(),
            json!({
                "type": "borrowLendUserState",
                "user": "0x2222222222222222222222222222222222222222"
            })
        );
        assert_eq!(
            payload_for_parser(
                "https://api.hyperliquid.xyz/info?dex=xyz",
                "hl_meta",
                &user,
                Some("SPCX")
            )
            .unwrap(),
            json!({ "type": "meta", "dex": "xyz" })
        );
        assert_eq!(
            payload_for_parser(
                "https://api.hyperliquid.xyz/info?dex=xyz",
                "hl_l2_book",
                &user,
                Some("SPCX")
            )
            .unwrap(),
            json!({ "type": "l2Book", "coin": "xyz:SPCX" })
        );
    }

    #[test]
    fn matches_hip3_prefixed_symbols_in_parsers() {
        let mids = json!({ "xyz:SPCX": "203.905" });
        assert_eq!(
            parse_all_mids_value(&mids, "xyz:SPCX").unwrap(),
            json!("203.905")
        );

        let meta_and_ctx = json!([
            {
                "universe": [
                    { "name": "xyz:SPCX", "maxLeverage": 5, "szDecimals": 2 }
                ],
                "collateralToken": 0
            },
            [{
                "funding": "0.00000625",
                "openInterest": "12945.4546",
                "markPx": "203.91",
                "oraclePx": "203.9",
                "midPx": "203.905",
                "premium": "0",
                "prevDayPx": "200",
                "dayNtlVlm": "1000000",
                "impactPxs": ["203.89", "203.95"]
            }]
        ]);
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "SPCX", HlAssetMetric::MarkPrice).unwrap(),
            json!("203.91")
        );

        let clearinghouse = json!({
            "assetPositions": [{
                "position": {
                    "coin": "xyz:SPCX",
                    "szi": "25.77",
                    "liquidationPx": "180.2199216574",
                    "unrealizedPnl": "32.9856",
                    "leverage": { "type": "isolated", "value": 5 },
                    "cumFunding": { "sinceOpen": "0.003908" }
                }
            }]
        });
        assert_eq!(
            parse_state_value(
                "hl_account",
                &FieldLocation::PerpLiqPrice {
                    position_id: "p".into()
                },
                "SPCX",
                &clearinghouse,
            )
            .unwrap(),
            json!("180.2199216574")
        );
    }

    #[test]
    fn routes_parser_ids_and_slots_to_action_live_values() {
        use crate::walker::ActionSlot;

        let clearinghouse = json!({
            "marginSummary": {
                "accountValue": "1000",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000",
                "totalMarginUsed": "200"
            },
            "crossMarginSummary": {
                "accountValue": "1000",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000",
                "totalMarginUsed": "200"
            },
            "crossMaintenanceMarginUsed": "50",
            "withdrawable": "800",
            "assetPositions": [{
                "type": "oneWay",
                "position": {
                    "coin": "BTC",
                    "szi": "0.1",
                    "leverage": { "type": "cross", "value": 5 },
                    "entryPx": "60000",
                    "positionValue": "6000",
                    "unrealizedPnl": "12.3",
                    "returnOnEquity": "0.02",
                    "liquidationPx": "50000",
                    "marginUsed": "1200",
                    "maxLeverage": 50,
                    "cumFunding": {
                        "allTime": "-1",
                        "sinceOpen": "-1",
                        "sinceChange": "0"
                    }
                }
            }],
            "time": 1_710_000_000_123_u64
        });
        let account_value = parse_live_input_value(
            "hl_account",
            &ActionSlot::PerpOpenUserAccountState,
            "BTC-USD",
            &clearinghouse,
        )
        .unwrap();
        assert_eq!(account_value["total_collateral_usd"], json!("0x3e8"));
        assert_eq!(account_value["used_margin_usd"], json!("0xc8"));
        assert_eq!(account_value["free_margin_usd"], json!("0x320"));
        assert_eq!(
            account_value["open_positions"][0][0]["symbol"],
            json!("BTC")
        );

        let open_orders = json!([
            { "coin": "BTC", "side": "B", "limitPx": "60000", "sz": "0.1", "oid": 7, "reduceOnly": false },
            { "coin": "ETH", "side": "A", "limitPx": "3000", "sz": "1", "oid": 8, "reduceOnly": false }
        ]);
        assert_eq!(
            parse_live_input_value(
                "hl_open_orders",
                &ActionSlot::PerpPlaceLimitOpenOrdersCount,
                "BTC-USD",
                &open_orders,
            )
            .unwrap(),
            json!(2u64)
        );

        let l2 = json!({
            "coin": "BTC",
            "time": 1_710_000_000_123_u64,
            "levels": [
                [{ "px": "59999", "sz": "1" }],
                [{ "px": "60002", "sz": "1" }]
            ]
        });
        assert_eq!(
            parse_live_input_value(
                "hl_l2_book",
                &ActionSlot::PerpPlaceLimitBestBidAsk,
                "BTC-USD",
                &l2,
            )
            .unwrap(),
            json!(["59999", "60002"])
        );
    }
}
