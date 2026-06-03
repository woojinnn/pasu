//! Runtime orchestrator for wallet-state and action live-input refresh.
//!
//! The orchestrator walks stale `LiveField`s, batches them by external source,
//! dispatches each batch to the matching fetcher, and writes successful results
//! back into the state or action being refreshed.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use policy_state::{
    Confidence, DataSource, LiveField, Position, PositionKind, Price, ProtocolRef, SignedI256,
    Time, WalletState,
};
use policy_transition::action::{Action, ActionBody, PerpAction};

use crate::batcher::{batch_by_source, BatchKind, FetchBatch};
use crate::calc::{CalcContext, CalcRegistry};
use crate::error::SyncError;
use crate::fetchers::onchain::OnchainCall;
use crate::fetchers::oracle::{provider_key, PriceFetcher, RestJsonOracleFetcher};
use crate::fetchers::{
    ChainlinkFetcher, HyperliquidFetcher, OnchainViewFetcher, RegistryFetcher, UniswapXFetcher,
    UniswapXOrder,
};
use crate::walker::{walk_stale, FieldLocation, WalkStats};

#[derive(Debug, Default, Clone)]
pub struct RefreshReport {
    pub walked: WalkStats,
    pub batches_processed: usize,
    pub fields_updated: usize,
    pub fields_failed: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct HyperliquidAccountReport {
    pub account_updated: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct IntentOrdersReport {
    pub orders_updated: usize,
    pub errors: Vec<String>,
}

pub struct Orchestrator {
    onchain: OnchainViewFetcher,
    // Normalized oracle provider key to fetcher.
    price_fetchers: HashMap<String, Arc<dyn PriceFetcher>>,
    registry: Option<RegistryFetcher>,
    hyperliquid: Option<HyperliquidFetcher>,
    uniswap_x: Option<UniswapXFetcher>,
    calc: CalcRegistry,
    // Global values used by derived live-field inputs.
    globals: crate::resolver::GlobalValues,
    // Direct router access for primitive sync and receipt watchers.
    router: Option<Arc<crate::RpcRouter>>,
}

impl Orchestrator {
    #[must_use]
    pub fn new(onchain: OnchainViewFetcher) -> Self {
        Self {
            onchain,
            price_fetchers: HashMap::new(),
            registry: None,
            hyperliquid: None,
            uniswap_x: None,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: None,
        }
    }

    pub fn set_global(&mut self, name: impl Into<String>, value: serde_json::Value) {
        self.globals.insert(name.into(), value);
    }

    pub(crate) fn router_ref(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    #[must_use]
    pub fn router_arc(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    pub fn with_price_fetcher(
        mut self,
        name: impl Into<String>,
        fetcher: Arc<dyn PriceFetcher>,
    ) -> Self {
        self.price_fetchers.insert(name.into(), fetcher);
        self
    }

    #[must_use]
    pub fn with_chainlink(self, chainlink: ChainlinkFetcher) -> Self {
        self.with_price_fetcher("chainlink", Arc::new(chainlink))
    }

    #[must_use]
    pub fn with_registry(mut self, registry: RegistryFetcher) -> Self {
        self.registry = Some(registry);
        self
    }

    #[must_use]
    pub fn with_hyperliquid(mut self, hl: HyperliquidFetcher) -> Self {
        self.hyperliquid = Some(hl);
        self
    }

    #[must_use]
    pub fn with_uniswap_x(mut self, uni: UniswapXFetcher) -> Self {
        self.uniswap_x = Some(uni);
        self
    }

    #[must_use]
    pub fn with_calc(mut self, calc: CalcRegistry) -> Self {
        self.calc = calc;
        self
    }

    #[must_use]
    pub fn from_rpc_router(router: Arc<crate::RpcRouter>) -> Self {
        let onchain = OnchainViewFetcher::new(router.clone());
        let chainlink = ChainlinkFetcher::new(router.clone());
        let mut price_fetchers: HashMap<String, Arc<dyn PriceFetcher>> = HashMap::new();
        price_fetchers.insert("chainlink".into(), Arc::new(chainlink));
        Self {
            onchain,
            price_fetchers,
            registry: Some(RegistryFetcher::new()),
            hyperliquid: Some(HyperliquidFetcher::new()),
            uniswap_x: None,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        }
    }

    /// - `RpcRouter` ← `cfg.rpc`
    pub fn from_sync_config(cfg: &crate::SyncConfig) -> Result<Self, SyncError> {
        let router = Arc::new(crate::RpcRouter::from_config(cfg.rpc.clone())?);
        let onchain = OnchainViewFetcher::new(router.clone());

        let mut price_fetchers: HashMap<String, Arc<dyn PriceFetcher>> = HashMap::new();

        // Chainlink (on-chain).
        let chainlink = ChainlinkFetcher::from_sync_config(router.clone(), &cfg.oracles.chainlink);
        price_fetchers.insert("chainlink".into(), Arc::new(chainlink));

        for (name, rest_cfg) in &cfg.oracles.rest {
            let f = RestJsonOracleFetcher::from_sync_config(name.clone(), rest_cfg);
            price_fetchers.insert(name.clone(), Arc::new(f));
        }

        let hyperliquid = cfg
            .venues
            .hyperliquid
            .as_ref()
            .map(HyperliquidFetcher::from_sync_config);
        let uniswap_x = cfg
            .venues
            .uniswap
            .as_ref()
            .map(UniswapXFetcher::from_sync_config);
        Ok(Self {
            onchain,
            price_fetchers,
            registry: Some(RegistryFetcher::new()),
            hyperliquid,
            uniswap_x,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        })
    }

    fn price_fetcher_for(
        &self,
        source: &policy_state::DataSource,
    ) -> Option<&Arc<dyn PriceFetcher>> {
        match source {
            policy_state::DataSource::OracleFeed { provider, .. } => {
                self.price_fetchers.get(&provider_key(provider))
            }
            _ => None,
        }
    }

    pub async fn refresh(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<RefreshReport, SyncError> {
        let (stale, walked) = walk_stale(state, now);
        let mut report = RefreshReport {
            walked,
            ..Default::default()
        };
        if stale.is_empty() {
            return Ok(report);
        }

        let batches = batch_by_source(stale);
        for batch in batches {
            report.batches_processed += 1;
            match self.process_batch(batch, state, now).await {
                Ok((ok, fail)) => {
                    report.fields_updated += ok;
                    report.fields_failed += fail;
                }
                Err(e) => {
                    report.errors.push(format!("{e}"));
                }
            }
        }
        Ok(report)
    }

    pub async fn sync_hyperliquid_account(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<HyperliquidAccountReport, SyncError> {
        let Some(hl) = self.hyperliquid.as_ref() else {
            return Ok(HyperliquidAccountReport {
                account_updated: false,
                errors: vec!["hyperliquid fetcher is not configured".into()],
            });
        };

        let user = state.wallet_id.address;
        let account = hl.fetch_account_snapshot("", &user).await?;
        let source = DataSource::VenueApi {
            endpoint: hl.info_endpoint(),
            parser_id: "hl_account".into(),
            auth: None,
        };
        upsert_hyperliquid_account(state, account, source, now)?;
        Ok(HyperliquidAccountReport {
            account_updated: true,
            errors: Vec::new(),
        })
    }

    /// Discover and reconcile `UniswapX` (intent) order status for this wallet.
    /// Venue is the source of truth: each configured chain is polled and the
    /// returned orders are upserted into `state.pending` (keyed by `orderHash`).
    pub async fn sync_intent_orders(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<IntentOrdersReport, SyncError> {
        let Some(uni) = self.uniswap_x.as_ref() else {
            return Ok(IntentOrdersReport {
                orders_updated: 0,
                errors: vec!["uniswap_x fetcher is not configured".into()],
            });
        };
        let swapper = state.wallet_id.address;
        let mut report = IntentOrdersReport::default();
        let reactor = uniswap_x_reactor();
        // v2 lists by swapper across all chains in one call; each order carries
        // its own chainId, so there is no per-chain loop.
        match uni.fetch_orders(&swapper).await {
            Ok(orders) => {
                report.orders_updated = orders.len();
                upsert_intent_orders(state, &orders, reactor, &swapper, now);
            }
            Err(e) => report.errors.push(format!("uniswap_x: {e}")),
        }
        Ok(report)
    }

    pub async fn refresh_action(
        &self,
        action: &mut policy_transition::action::Action,
        state: &WalletState,
        now: Time,
    ) -> Result<RefreshReport, SyncError> {
        let (stale, walked) = crate::action_walk::walk_action_stale(action, now);
        let mut report = RefreshReport {
            walked,
            ..Default::default()
        };
        if stale.is_empty() {
            return Ok(report);
        }

        let batches = batch_by_source(stale);
        for batch in batches {
            report.batches_processed += 1;
            match self
                .process_batch_for_action(batch, action, state, now)
                .await
            {
                Ok((ok, fail)) => {
                    report.fields_updated += ok;
                    report.fields_failed += fail;
                }
                Err(e) => {
                    report.errors.push(format!("{e}"));
                }
            }
        }
        Ok(report)
    }

    async fn process_batch_for_action(
        &self,
        batch: FetchBatch,
        action: &mut policy_transition::action::Action,
        state: &WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        let mut ok = 0usize;
        let mut fail = 0usize;
        match &batch.kind {
            BatchKind::Oracle => {
                for item in batch.items {
                    let Some(fetcher) = self.price_fetcher_for(&item.source) else {
                        fail += 1;
                        continue;
                    };
                    match fetcher.fetch_price(&item.source).await {
                        Ok(price) => {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                serde_json::Value::String(price.0),
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Onchain { chain } => {
                let calls: Result<Vec<_>, _> = batch
                    .items
                    .iter()
                    .map(|item| {
                        let args = match &item.location {
                            crate::walker::FieldLocation::Action { slot, .. } => {
                                crate::args_resolver::resolve_args(slot, action, state)
                            }
                            _ => Vec::new(),
                        };
                        crate::fetchers::onchain::OnchainCall::from_source(&item.source, args)
                    })
                    .collect();
                let Ok(calls) = calls else {
                    return Ok((0, batch.items.len()));
                };
                let outcomes = self.onchain.fetch_batch(chain, &calls).await?;
                for (item, outcome) in batch.items.into_iter().zip(outcomes.into_iter()) {
                    if outcome.success {
                        if let Some(value) = outcome.value {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                value,
                                now,
                            );
                            ok += 1;
                        } else {
                            fail += 1;
                        }
                    } else {
                        fail += 1;
                    }
                }
            }
            BatchKind::Registry { .. } => {
                let Some(reg) = self.registry.as_ref() else {
                    return Ok((0, batch.items.len()));
                };
                for item in batch.items {
                    match reg.fetch(&item.source).await {
                        Ok(v) => {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                v,
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Venue { endpoint } => {
                let is_hl = is_hyperliquid_endpoint(endpoint);
                let Some(hl) = (if is_hl {
                    self.hyperliquid.as_ref()
                } else {
                    None
                }) else {
                    return Ok((0, batch.items.len()));
                };
                for item in batch.items {
                    let FieldLocation::Action { slot, .. } = &item.location else {
                        fail += 1;
                        continue;
                    };
                    let market_symbol =
                        action_market_symbol(action, state, &item.location).unwrap_or_default();
                    match hl
                        .fetch_action_value(
                            &item.source,
                            slot,
                            &market_symbol,
                            &state.wallet_id.address,
                        )
                        .await
                    {
                        Ok(v) => {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                v,
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Derived | BatchKind::UserSupplied => {}
        }
        Ok((ok, fail))
    }

    pub(crate) async fn process_batch_public(
        &self,
        batch: FetchBatch,
        state: &mut WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        self.process_batch(batch, state, now).await
    }

    async fn process_batch(
        &self,
        batch: FetchBatch,
        state: &mut WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        match batch.kind {
            BatchKind::Onchain { chain } => {
                // State-level on-chain live fields only support no-arg calls.
                // Action refresh resolves call arguments through `actions::args`.
                let calls: Result<Vec<OnchainCall>, _> = batch
                    .items
                    .iter()
                    .map(|item| OnchainCall::from_source(&item.source, vec![]))
                    .collect();
                let calls = calls?;

                let outcomes = self.onchain.fetch_batch(&chain, &calls).await?;

                let mut ok = 0;
                let mut fail = 0;
                for (item, outcome) in batch.items.into_iter().zip(outcomes.into_iter()) {
                    if outcome.success {
                        if let Some(value) = outcome.value {
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        } else {
                            fail += 1;
                        }
                    } else {
                        fail += 1;
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Oracle => {
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    let Some(fetcher) = self.price_fetcher_for(&item.source) else {
                        fail += 1;
                        continue;
                    };
                    match fetcher.fetch_price(&item.source).await {
                        Ok(price) => {
                            apply_value(
                                state,
                                &item.location,
                                serde_json::Value::String(price.0),
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Registry { .. } => {
                let registry = match self.registry.as_ref() {
                    Some(r) => r,
                    None => return Ok((0, batch.items.len())),
                };
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    match registry.fetch(&item.source).await {
                        Ok(value) => {
                            // Registry values can be assigned directly to the
                            // requested live-field location.
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Derived => {
                // Derived fields in one batch are assumed independent. Callers
                // rerun refresh for multi-layer derived dependencies.
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    if let policy_state::DataSource::DerivedFrom { calc_id, inputs } = &item.source
                    {
                        let resolved =
                            crate::resolver::resolve_inputs(state, &self.globals, inputs);
                        let ctx = CalcContext {
                            state,
                            inputs: resolved,
                        };
                        match self.calc.run(calc_id, &ctx) {
                            Ok(value) => {
                                apply_value(state, &item.location, value, now);
                                ok += 1;
                            }
                            Err(_) => fail += 1,
                        }
                    } else {
                        fail += 1;
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Venue { endpoint } => {
                // Endpoint matching currently routes venue live fields to the
                // Hyperliquid fetcher.
                let is_hl = is_hyperliquid_endpoint(&endpoint);
                let hl = if is_hl {
                    self.hyperliquid.as_ref()
                } else {
                    None
                };
                let hl = match hl {
                    Some(h) => h,
                    None => return Ok((0, batch.items.len())),
                };
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    let fetched = match state_market_symbol(state, &item.location) {
                        Some(market_symbol) => {
                            hl.fetch_state_value(
                                &item.source,
                                &item.location,
                                &market_symbol,
                                &state.wallet_id.address,
                            )
                            .await
                        }
                        None => hl.fetch(&item.source).await,
                    };
                    match fetched {
                        Ok(value) => {
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::UserSupplied => Ok((0, 0)),
        }
    }
}

fn apply_value(state: &mut WalletState, loc: &FieldLocation, value: Value, now: Time) {
    match loc {
        FieldLocation::TokenPrice { token_key_json } => {
            if let Ok(key) = serde_json::from_str::<policy_state::TokenKey>(token_key_json) {
                if let Some(holding) = state.tokens.get_mut(&key) {
                    if let Some(price) = holding.price_usd.as_mut() {
                        if let Some(p) = value_to_price(&value) {
                            price.value = p;
                            price.synced_at = now;
                            price.confidence = Some(Confidence::fresh());
                        }
                    }
                }
            }
        }
        FieldLocation::LendingHealthFactor { position_id } => {
            if let Some(field) = lending_field_mut(state, position_id, LendingMetric::Hf) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::LendingLtv { position_id } => {
            if let Some(field) = lending_field_mut(state, position_id, LendingMetric::Ltv) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::LendingLiquidationThreshold { position_id } => {
            if let Some(field) = lending_field_mut(state, position_id, LendingMetric::LiqThr) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::PerpMarkPrice { position_id } => {
            if let Some(price) = perp_position_mut(state, position_id).map(|p| &mut p.mark_price) {
                if let Some(p) = value_to_price(&value) {
                    price.value = p;
                    price.synced_at = now;
                    price.confidence = Some(Confidence::fresh());
                }
            }
        }
        FieldLocation::PerpLiqPrice { position_id } => {
            if let Some(field) = perp_position_mut(state, position_id).map(|p| &mut p.liq_price) {
                match &value {
                    Value::Null => {
                        field.value = None;
                        field.synced_at = now;
                        field.confidence = Some(Confidence::fresh());
                    }
                    _ => {
                        if let Some(p) = value_to_price(&value) {
                            field.value = Some(p);
                            field.synced_at = now;
                            field.confidence = Some(Confidence::fresh());
                        }
                    }
                }
            }
        }
        FieldLocation::PerpUnrealizedPnl { position_id } => {
            if let Some(field) =
                perp_position_mut(state, position_id).map(|p| &mut p.unrealized_pnl)
            {
                if let Some(v) = value_to_i256(&value) {
                    field.value = v;
                    field.synced_at = now;
                    field.confidence = Some(Confidence::fresh());
                }
            }
        }
        FieldLocation::PerpFundingOwed { position_id } => {
            if let Some(field) = perp_position_mut(state, position_id).map(|p| &mut p.funding_owed)
            {
                if let Some(v) = value_to_i256(&value) {
                    field.value = v;
                    field.synced_at = now;
                    field.confidence = Some(Confidence::fresh());
                }
            }
        }
        FieldLocation::PerpLeverage { position_id } => {
            if let Some(field) = perp_position_mut(state, position_id).map(|p| &mut p.leverage) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::Action { .. } => {}
    }
}

fn value_to_price(v: &Value) -> Option<Price> {
    match v {
        Value::String(s) => Some(policy_state::Decimal::new(s.clone())),
        Value::Number(n) => Some(policy_state::Decimal::new(n.to_string())),
        _ => None,
    }
}

fn value_to_i256(v: &Value) -> Option<SignedI256> {
    use std::str::FromStr;
    match v {
        Value::String(s) => SignedI256::from_str(s).ok(),
        Value::Number(n) => n.as_i64().and_then(|i| SignedI256::try_from(i).ok()),
        _ => None,
    }
}

fn set_decimal(field: &mut LiveField<policy_state::Decimal>, v: &Value, now: Time) {
    if let Some(d) = value_to_price(v) {
        field.value = d;
        field.synced_at = now;
        field.confidence = Some(Confidence::fresh());
    }
}

enum LendingMetric {
    Hf,
    Ltv,
    LiqThr,
}

fn lending_field_mut<'a>(
    state: &'a mut WalletState,
    position_id: &str,
    metric: LendingMetric,
) -> Option<&'a mut LiveField<policy_state::Decimal>> {
    let pos = state.positions.iter_mut().find(|p| p.id == position_id)?;
    match &mut pos.kind {
        PositionKind::LendingAccount(la) => Some(match metric {
            LendingMetric::Hf => &mut la.health_factor,
            LendingMetric::Ltv => &mut la.ltv,
            LendingMetric::LiqThr => &mut la.liquidation_threshold,
        }),
        _ => None,
    }
}

fn perp_position_mut<'a>(
    state: &'a mut WalletState,
    position_id: &str,
) -> Option<&'a mut policy_state::PerpPosition> {
    let pos = state.positions.iter_mut().find(|p| p.id == position_id)?;
    match &mut pos.kind {
        PositionKind::PerpPosition(p) => Some(p),
        _ => None,
    }
}

const HL_ACCOUNT_ID: &str = "hyperliquid/account";

fn upsert_hyperliquid_account(
    state: &mut WalletState,
    account: policy_state::HlAccount,
    source: DataSource,
    now: Time,
) -> Result<(), SyncError> {
    let position = Position {
        id: HL_ACCOUNT_ID.to_owned(),
        protocol: ProtocolRef::new("hyperliquid"),
        chain: None,
        kind: PositionKind::HyperliquidAccount(account),
        primitives_synced_at: now,
        primitives_source: source,
    };

    if let Some(existing) = state.positions.iter_mut().find(|p| p.id == HL_ACCOUNT_ID) {
        if !matches!(existing.kind, PositionKind::HyperliquidAccount(_)) {
            return Err(SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("{HL_ACCOUNT_ID} exists but is not a HyperliquidAccount"),
            });
        }
        *existing = position;
    } else {
        state.positions.push(position);
    }
    Ok(())
}

/// `UniswapX` V2 reactor on Ethereum mainnet (the permit-cap spender). Per-chain
/// reactors can be threaded through config later (spec §12).
fn uniswap_x_reactor() -> policy_state::Address {
    use std::str::FromStr;
    policy_state::Address::from_str("0x00000011f84b9aa48e5f8aa8b9897600006289be")
        .unwrap_or(policy_state::Address::ZERO)
}

/// Upsert discovered `UniswapX` orders into `state.pending`, keyed by the venue
/// `orderHash` embedded in `PendingTx.id`. Existing entries are replaced in
/// place (status transitions); new ones are appended.
pub(crate) fn upsert_intent_orders(
    state: &mut WalletState,
    orders: &[UniswapXOrder],
    reactor: policy_state::Address,
    swapper: &policy_state::Address,
    now: Time,
) {
    use policy_state::pending::PendingStatus;
    for order in orders {
        let pending = order.to_pending_tx(reactor, swapper, now);
        // Terminal orders are pruned from `pending` — filled / cancelled /
        // expired / failed no longer need tracking. Active ones are upserted in
        // place (status transitions) or appended.
        let terminal = matches!(
            pending.lifecycle.status,
            PendingStatus::Filled
                | PendingStatus::Cancelled
                | PendingStatus::Expired
                | PendingStatus::Failed
        );
        if terminal {
            state.pending.retain(|p| p.id != pending.id);
        } else if let Some(existing) = state.pending.iter_mut().find(|p| p.id == pending.id) {
            *existing = pending;
        } else {
            state.pending.push(pending);
        }
    }
}

fn is_hyperliquid_endpoint(endpoint: &str) -> bool {
    endpoint.is_empty()
        || endpoint.contains("hyperliquid")
        || endpoint == "https://api.hyperliquid.xyz/info"
}

fn state_market_symbol(state: &WalletState, location: &FieldLocation) -> Option<String> {
    match location {
        FieldLocation::PerpMarkPrice { position_id }
        | FieldLocation::PerpLiqPrice { position_id }
        | FieldLocation::PerpUnrealizedPnl { position_id }
        | FieldLocation::PerpFundingOwed { position_id }
        | FieldLocation::PerpLeverage { position_id } => state
            .positions
            .iter()
            .find(|p| p.id == *position_id)
            .and_then(|p| match &p.kind {
                PositionKind::PerpPosition(perp) => Some(perp.market.symbol.clone()),
                _ => None,
            }),
        _ => None,
    }
}

fn action_market_symbol(
    action: &Action,
    state: &WalletState,
    location: &FieldLocation,
) -> Option<String> {
    let FieldLocation::Action { action_index, .. } = location else {
        return None;
    };
    let body = body_at_index(&action.body, *action_index)?;
    let ActionBody::Perp(perp) = body else {
        return None;
    };
    perp_action_market_symbol(perp, state)
}

fn body_at_index(body: &ActionBody, index: usize) -> Option<&ActionBody> {
    match body {
        ActionBody::Multicall { actions } => actions.get(index),
        single if index == 0 => Some(single),
        _ => None,
    }
}

fn perp_action_market_symbol(perp: &PerpAction, state: &WalletState) -> Option<String> {
    match perp {
        PerpAction::OpenPosition(a) => Some(a.market.symbol.clone()),
        PerpAction::ClosePosition(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::IncreasePosition(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::DecreasePosition(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::AdjustMargin(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::ChangeLeverage(a) => Some(a.market.symbol.clone()),
        PerpAction::ChangeMarginMode(a) => Some(a.market.symbol.clone()),
        PerpAction::PlaceLimitOrder(a) => Some(a.market.symbol.clone()),
        PerpAction::PlaceStopOrder(a) => Some(a.market.symbol.clone()),
        PerpAction::CancelOrder(_) => None,
        PerpAction::ClaimFunding(a) => a.market.as_ref().map(|m| m.symbol.clone()),
    }
}

fn state_position_market_symbol(state: &WalletState, position_id: &str) -> Option<String> {
    state
        .positions
        .iter()
        .find(|p| p.id == position_id)
        .and_then(|p| match &p.kind {
            PositionKind::PerpPosition(perp) => Some(perp.market.symbol.clone()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId};

    #[test]
    fn upsert_intent_orders_tracks_active_and_prunes_on_terminal() {
        use crate::fetchers::UniswapXOrder;
        use policy_state::pending::PendingStatus;
        use policy_state::{WalletId, U256};

        let reactor = Address::ZERO;
        let swapper = Address::ZERO;
        let now = Time::from_unix(1_738_000_000);

        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));

        // Round 1: one open order is discovered → added as Active.
        let open = UniswapXOrder {
            order_hash: "0xhash1".into(),
            order_status: "open".into(),
            order_type: "Dutch_V2".into(),
            chain_id: 1,
            deadline: Some(1_738_003_600),
            sell_token: Address::ZERO,
            sell_amount: U256::from(600u64),
            buy_token: Address::ZERO,
            buy_min: U256::from(1u64),
        };
        super::upsert_intent_orders(
            &mut state,
            std::slice::from_ref(&open),
            reactor,
            &swapper,
            now,
        );
        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.pending[0].id, "intent:uniswap_x:0xhash1");
        assert_eq!(state.pending[0].lifecycle.status, PendingStatus::Active);

        // Round 2: same hash now filled → pruned from pending (terminal cleanup).
        let filled = UniswapXOrder {
            order_status: "filled".into(),
            ..open
        };
        super::upsert_intent_orders(&mut state, &[filled], reactor, &swapper, now);
        assert!(
            state.pending.is_empty(),
            "terminal order pruned from pending"
        );
    }

    #[tokio::test]
    async fn refresh_empty_state_is_noop() {
        let toml = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let orch = Orchestrator::from_rpc_router(router);

        let mut state = WalletState::new(policy_state::WalletId::new(
            Address::ZERO,
            [ChainId::ethereum_mainnet()],
        ));
        let report = orch.refresh(&mut state, Time::from_unix(0)).await.unwrap();
        assert_eq!(report.walked.total_live_fields, 0);
        assert_eq!(report.fields_updated, 0);
        assert_eq!(report.batches_processed, 0);
    }

    #[tokio::test]
    async fn derived_hf_computes_from_globals() {
        use policy_state::{
            DataSource, Decimal, Duration, FieldRef, LendingAccount, LiveField, MarketRef,
            Position, PositionKind, Time as T, VenueRef, WalletId,
        };

        let toml = r#"
[chains."eip155:1"]
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let mut orch = Orchestrator::from_rpc_router(router);

        // collateral=1000, debt=500, liq_threshold=0.8 → HF = (1000*0.8)/500 = 1.6
        orch.set_global("collateral_usd", serde_json::json!("1000"));
        orch.set_global("debt_usd", serde_json::json!("500"));
        orch.set_global("liq_threshold", serde_json::json!("0.8"));

        let hf_source = DataSource::DerivedFrom {
            calc_id: "aave_hf".into(),
            inputs: vec![
                FieldRef::Global {
                    name: "collateral_usd".into(),
                },
                FieldRef::Global {
                    name: "debt_usd".into(),
                },
                FieldRef::Global {
                    name: "liq_threshold".into(),
                },
            ],
        };

        let stale_at = T::from_unix(0);
        let now = T::from_unix(10_000);
        let fresh_source = DataSource::UserSupplied;

        let lending = LendingAccount {
            market: MarketRef {
                symbol: "aave-v3".into(),
                venue: VenueRef::new("aave"),
            },
            collaterals: vec![],
            debts: vec![],
            emode: None,
            is_isolated: false,
            health_factor: LiveField::new(Decimal::new("0"), hf_source, stale_at)
                .with_ttl(Duration::from_secs(60)),
            ltv: LiveField::new(Decimal::new("0"), fresh_source.clone(), now)
                .with_ttl(Duration::from_secs(60)),
            liquidation_threshold: LiveField::new(Decimal::new("0.8"), fresh_source, now)
                .with_ttl(Duration::from_secs(60)),
        };

        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        state.positions.push(Position {
            id: "aave_v3:main".into(),
            protocol: policy_state::ProtocolRef::new("aave_v3"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::LendingAccount(lending),
            primitives_synced_at: now,
            primitives_source: DataSource::UserSupplied,
        });

        let report = orch.refresh(&mut state, now).await.unwrap();
        assert!(report.fields_updated >= 1, "HF should have been updated");

        if let PositionKind::LendingAccount(la) = &state.positions[0].kind {
            assert_eq!(la.health_factor.value.as_str(), "1.6");
        } else {
            panic!("expected lending account");
        }
    }

    #[tokio::test]
    async fn sync_hyperliquid_account_replaces_local_account_with_snapshot() {
        use std::str::FromStr;

        use policy_state::{
            DataSource, Decimal, HlAccount, HlBorrowLendAccount, HlBorrowLendBalance,
            HlBorrowLendTokenState, HlOpenOrder, HlSpotBalance, HlStakingAccount, HlVaultEquity,
            Position, PositionKind, ProtocolRef, Time as T, WalletId,
        };
        use serde_json::{json, Value};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        async fn read_request_json(stream: &mut TcpStream) -> Value {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let n = stream.read(&mut tmp).await.unwrap();
                assert!(n > 0, "connection closed before request body");
                buf.extend_from_slice(&tmp[..n]);
                let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&buf[..header_end]);
                let len = headers
                    .lines()
                    .find_map(|line| {
                        let lower = line.to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if buf.len() >= header_end + len {
                    return serde_json::from_slice(&buf[header_end..header_end + len]).unwrap();
                }
            }
        }

        async fn write_json(stream: &mut TcpStream, body: Value) {
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }

        async fn spawn_hl_info_server() -> String {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    tokio::spawn(async move {
                        let req = read_request_json(&mut stream).await;
                        let dex = req.get("dex").and_then(Value::as_str);
                        let body = match (req["type"].as_str().unwrap_or_default(), dex) {
                            ("clearinghouseState", Some("xyz")) => json!({
                                "marginSummary": {
                                    "accountValue": "1077.754757",
                                    "totalNtlPos": "5257.5954",
                                    "totalRawUsd": "-4179.840643",
                                    "totalMarginUsed": "1077.754757"
                                },
                                "crossMarginSummary": {
                                    "accountValue": "1077.754757",
                                    "totalNtlPos": "5257.5954",
                                    "totalRawUsd": "-4179.840643",
                                    "totalMarginUsed": "1077.754757"
                                },
                                "crossMaintenanceMarginUsed": "0",
                                "withdrawable": "0",
                                "assetPositions": [{
                                    "type": "oneWay",
                                    "position": {
                                        "coin": "xyz:SPCX",
                                        "szi": "25.77",
                                        "leverage": { "type": "isolated", "value": 5, "rawUsd": "-4179.840643" },
                                        "entryPx": "202.74",
                                        "positionValue": "5257.5954",
                                        "unrealizedPnl": "32.9856",
                                        "returnOnEquity": "0.033",
                                        "liquidationPx": "180.2199216574",
                                        "marginUsed": "1077.754757",
                                        "maxLeverage": 5,
                                        "cumFunding": {
                                            "allTime": "0.003908",
                                            "sinceOpen": "0.003908",
                                            "sinceChange": "0.003908"
                                        }
                                    }
                                }],
                                "time": 1_710_000_000_123_u64
                            }),
                            ("clearinghouseState", None) => json!({
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
                                        "unrealizedPnl": "12",
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
                            }),
                            ("frontendOpenOrders", Some("xyz")) => json!([{
                                "timestamp": 1_780_211_477_428_u64,
                                "coin": "xyz:SPCX",
                                "side": "A",
                                "limitPx": "170.2",
                                "sz": "0.0",
                                "oid": 449_792_035_550_u64,
                                "origSz": "0.0",
                                "cloid": null,
                                "orderType": "Stop Market",
                                "tif": null,
                                "reduceOnly": true,
                                "triggerCondition": "Price below 185",
                                "isTrigger": true,
                                "triggerPx": "185.0",
                                "children": [],
                                "isPositionTpsl": true
                            }]),
                            ("frontendOpenOrders", None) => json!([{
                                "timestamp": 1_710_000_000_124_u64,
                                "coin": "ETH",
                                "side": "B",
                                "limitPx": "3000",
                                "sz": "0.25",
                                "oid": 42,
                                "origSz": "0.25",
                                "cloid": null,
                                "orderType": "Limit",
                                "tif": "Gtc",
                                "reduceOnly": false
                            }]),
                            ("extraAgents", None) => json!([{
                                "name": "bot",
                                "address": "0x1111111111111111111111111111111111111111",
                                "validUntil": 1_710_000_000_999_u64
                            }]),
                            ("spotClearinghouseState", None) => json!({
                                "balances": [{
                                    "coin": "USDC",
                                    "token": 0,
                                    "total": "1125.961894",
                                    "hold": "1077.497057",
                                    "entryNtl": "0.0"
                                }],
                                "tokenToAvailableAfterMaintenance": [[0, "48.464837"]]
                            }),
                            ("delegatorSummary", None) => json!({
                                "delegated": "0.0",
                                "undelegated": "0.0",
                                "totalPendingWithdrawal": "46.84529183",
                                "nPendingWithdrawals": 1
                            }),
                            ("delegations", None) => json!([]),
                            ("userVaultEquities", None) => json!([{
                                "vaultAddress": "0x3333333333333333333333333333333333333333",
                                "equity": "742500.082809",
                                "lockedUntilTimestamp": 1_741_132_800_000_u64
                            }]),
                            ("borrowLendUserState", None) => json!({
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
                            }),
                            ("meta", Some("xyz")) => json!({
                                "universe": [
                                    { "name": "xyz:SPCX", "maxLeverage": 5, "szDecimals": 2 }
                                ],
                                "collateralToken": 0
                            }),
                            ("meta", None) => json!({
                                "universe": [
                                    { "name": "BTC", "maxLeverage": 50, "szDecimals": 5 },
                                    { "name": "ETH", "maxLeverage": 25, "szDecimals": 4 }
                                ],
                                "collateralToken": 0
                            }),
                            ("perpDexs", None) => json!([{ "name": "xyz", "fullName": "XYZ" }]),
                            (other, dex) => panic!("unexpected info request: {other}/{dex:?}"),
                        };
                        write_json(&mut stream, body).await;
                    });
                }
            });
            format!("http://{addr}")
        }

        let toml = r#"
[chains."eip155:1"]
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let base_url = spawn_hl_info_server().await;
        let orch = Orchestrator::from_rpc_router(router)
            .with_hyperliquid(crate::fetchers::HyperliquidFetcher::with_base_url(base_url));

        let now = T::from_unix(10_000);
        let user = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();
        let mut state =
            policy_state::WalletState::new(WalletId::new(user, [ChainId::ethereum_mainnet()]));
        state.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_usdc: Some(Decimal::new("10")),
                pending_outflow: Decimal::new("99"),
                positions: Vec::new(),
                open_orders: vec![HlOpenOrder {
                    asset_index: 99,
                    symbol: Some("OLD".to_owned()),
                    is_buy: false,
                    price: Decimal::new("1"),
                    size: Decimal::new("1"),
                    reduce_only: false,
                    tif: "gtc".to_owned(),
                    oid: Some(1),
                    order_type: None,
                    is_trigger: None,
                    trigger_price: None,
                    trigger_condition: None,
                    is_position_tpsl: None,
                }],
                spot_balances: vec![HlSpotBalance {
                    coin: "OLD".to_owned(),
                    token: 999,
                    total: Decimal::new("1"),
                    hold: Decimal::new("1"),
                    entry_ntl: Decimal::new("1"),
                    available_after_maintenance: None,
                }],
                staking: Some(HlStakingAccount {
                    delegated: Decimal::new("1"),
                    undelegated: Decimal::new("1"),
                    total_pending_withdrawal: Decimal::new("1"),
                    n_pending_withdrawals: 1,
                    delegations: Vec::new(),
                }),
                vault_equities: vec![HlVaultEquity {
                    vault_address: Address::from([0x99; 20]),
                    equity: Decimal::new("1"),
                    locked_until_timestamp: None,
                }],
                borrow_lend: Some(HlBorrowLendAccount {
                    token_states: vec![HlBorrowLendTokenState {
                        token: 999,
                        borrow: HlBorrowLendBalance {
                            basis: Decimal::new("1"),
                            value: Decimal::new("1"),
                        },
                        supply: HlBorrowLendBalance {
                            basis: Decimal::new("1"),
                            value: Decimal::new("1"),
                        },
                    }],
                    health: Some("old".to_owned()),
                    health_factor: Some(Decimal::new("1")),
                }),
                ..HlAccount::default()
            }),
            primitives_synced_at: T::from_unix(0),
            primitives_source: DataSource::UserSupplied,
        });

        let report = orch
            .sync_hyperliquid_account(&mut state, now)
            .await
            .unwrap();
        assert!(report.account_updated);

        let account = state
            .positions
            .iter()
            .find_map(|p| match &p.kind {
                PositionKind::HyperliquidAccount(a) if p.id == HL_ACCOUNT_ID => Some(a),
                _ => None,
            })
            .unwrap();
        assert_eq!(account.perp_usdc, Some(Decimal::new("800")));
        assert_eq!(account.pending_outflow, Decimal::new("0"));
        assert_eq!(account.positions.len(), 2);
        assert_eq!(account.positions[0].symbol.as_deref(), Some("BTC"));
        assert_eq!(account.positions[1].symbol.as_deref(), Some("xyz:SPCX"));
        assert_eq!(account.open_orders.len(), 2);
        assert_eq!(account.open_orders[0].symbol.as_deref(), Some("ETH"));
        assert_eq!(account.open_orders[0].oid, Some(42));
        assert_eq!(account.open_orders[1].symbol.as_deref(), Some("xyz:SPCX"));
        assert_eq!(
            account.open_orders[1].order_type.as_deref(),
            Some("Stop Market")
        );
        assert_eq!(account.open_orders[1].is_trigger, Some(true));
        assert_eq!(
            account.open_orders[1].trigger_price,
            Some(Decimal::new("185"))
        );
        assert_eq!(
            account.open_orders[1].trigger_condition.as_deref(),
            Some("Price below 185")
        );
        assert_eq!(account.open_orders[1].is_position_tpsl, Some(true));
        assert_eq!(account.spot_balances.len(), 1);
        assert_eq!(account.spot_balances[0].coin, "USDC");
        assert_eq!(
            account.spot_balances[0].available_after_maintenance,
            Some(Decimal::new("48.464837"))
        );
        let staking = account.staking.as_ref().unwrap();
        assert_eq!(
            staking.total_pending_withdrawal,
            Decimal::new("46.84529183")
        );
        assert_eq!(staking.n_pending_withdrawals, 1);
        assert!(staking.delegations.is_empty());
        assert_eq!(account.vault_equities.len(), 1);
        assert_eq!(
            account.vault_equities[0].equity,
            Decimal::new("742500.082809")
        );
        assert_eq!(
            account.vault_equities[0].locked_until_timestamp,
            Some(1_741_132_800_000_u64)
        );
        let borrow_lend = account.borrow_lend.as_ref().unwrap();
        assert_eq!(borrow_lend.health.as_deref(), Some("healthy"));
        assert_eq!(borrow_lend.token_states.len(), 1);
        assert_eq!(
            borrow_lend.token_states[0].supply.value,
            Decimal::new("44.69692314")
        );
        assert_eq!(account.agents.len(), 1);
    }
}
