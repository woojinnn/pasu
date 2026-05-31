//! Orchestrator — walker + batcher + fetchers 를 묶어 한 `WalletState` 를 신선화.
//!
//! 일반 흐름:
//! 1. [`walk_stale`] 로 stale `LiveField` 수집
//! 2. [`batch_by_source`] 로 source 별 묶음
//! 3. 각 batch 를 해당 fetcher 로 dispatch
//! 4. 결과 (`Value`) 를 다시 state 의 `LiveField` 에 write back
//!
//! Phase 4 에선 `OnchainView` 만 wired up. 나머지 (Oracle/Venue/Registry/Derived) 는
//! 후속 phase 에서 차례로.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use simulation_reducer::action::{Action, ActionBody, PerpAction};
use simulation_state::{
    Confidence, DataSource, LiveField, Position, PositionKind, Price, ProtocolRef, SignedI256,
    Time, WalletState,
};

use crate::batcher::{batch_by_source, BatchKind, FetchBatch};
use crate::calc::{CalcContext, CalcRegistry};
use crate::error::SyncError;
use crate::fetchers::onchain::OnchainCall;
use crate::fetchers::oracle::{provider_key, PriceFetcher, RestJsonOracleFetcher};
use crate::fetchers::{ChainlinkFetcher, HyperliquidFetcher, OnchainViewFetcher, RegistryFetcher};
use crate::walker::{walk_stale, FieldLocation, WalkStats};

/// 한 wallet refresh 결과 요약 — 디버깅 / 메트릭용.
#[derive(Debug, Default, Clone)]
pub struct RefreshReport {
    pub walked: WalkStats,
    pub batches_processed: usize,
    pub fields_updated: usize,
    pub fields_failed: usize,
    pub errors: Vec<String>,
}

/// Hyperliquid account snapshot refresh 결과.
#[derive(Debug, Default, Clone)]
pub struct HyperliquidAccountReport {
    pub account_updated: bool,
    pub errors: Vec<String>,
}

pub struct Orchestrator {
    onchain: OnchainViewFetcher,
    /// `provider_key` → fetcher. `provider_key` 는
    /// [`crate::fetchers::oracle::provider_key`] 로 정규화된 문자열.
    /// 예: `"chainlink"`, `"coingecko"`, `"pyth"`, `"redstone"`, ...
    ///
    /// Chainlink (on-chain) 든 REST oracle 이든 모두 같은 trait object 로 들어감.
    price_fetchers: HashMap<String, Arc<dyn PriceFetcher>>,
    registry: Option<RegistryFetcher>,
    hyperliquid: Option<HyperliquidFetcher>,
    calc: CalcRegistry,
    /// Global `LiveField` 값 (`gas_price`, `eth_usd` 등). `DerivedFrom` 의 Global `FieldRef`
    /// resolve 에 사용. scheduler/sync 가 주기적으로 갱신.
    globals: crate::resolver::GlobalValues,
    /// primitives sync (`balance/block_height/approval`) 용 직접 router 접근.
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
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: None,
        }
    }

    /// Global `LiveField` 값 갱신 (`gas_price`, `eth_usd` 등).
    pub fn set_global(&mut self, name: impl Into<String>, value: serde_json::Value) {
        self.globals.insert(name.into(), value);
    }

    /// primitives sync 가 사용할 router 참조.
    pub(crate) fn router_ref(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    /// 외부에서 (예: receipt watcher) 직접 RPC 호출이 필요할 때.
    #[must_use]
    pub fn router_arc(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    /// 임의 provider name 에 `PriceFetcher` 등록. dispatch 시 [`provider_key`] 가
    /// 반환하는 문자열과 일치해야 매칭됨.
    pub fn with_price_fetcher(
        mut self,
        name: impl Into<String>,
        fetcher: Arc<dyn PriceFetcher>,
    ) -> Self {
        self.price_fetchers.insert(name.into(), fetcher);
        self
    }

    /// Chainlink fetcher 를 "chainlink" 키로 등록.
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
    pub fn with_calc(mut self, calc: CalcRegistry) -> Self {
        self.calc = calc;
        self
    }

    /// router 만으로 minimal 구성 — Chainlink registry 와 Hyperliquid endpoint 는
    /// 기본값. 실 운영에서는 [`Self::from_sync_config`] 사용 권장.
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
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        }
    }

    /// `scopeball-sync.toml` (= [`crate::SyncConfig`]) 한 방으로 모든 fetcher 와이어링.
    ///
    /// - `RpcRouter` ← `cfg.rpc`
    /// - `ChainlinkFetcher` ← `cfg.oracles.chainlink` ("chainlink" 키)
    /// - `RestJsonOracleFetcher` × N ← `cfg.oracles.rest` (각 키 그대로)
    /// - `HyperliquidFetcher` ← `cfg.venues.hyperliquid` (있을 때만)
    /// - `RegistryFetcher` 는 stub
    pub fn from_sync_config(cfg: &crate::SyncConfig) -> Result<Self, SyncError> {
        let router = Arc::new(crate::RpcRouter::from_config(cfg.rpc.clone())?);
        let onchain = OnchainViewFetcher::new(router.clone());

        let mut price_fetchers: HashMap<String, Arc<dyn PriceFetcher>> = HashMap::new();

        // Chainlink (on-chain).
        let chainlink = ChainlinkFetcher::from_sync_config(router.clone(), &cfg.oracles.chainlink);
        price_fetchers.insert("chainlink".into(), Arc::new(chainlink));

        // REST oracles — 각 [oracles.rest.<name>] 블록당 fetcher 하나.
        for (name, rest_cfg) in &cfg.oracles.rest {
            let f = RestJsonOracleFetcher::from_sync_config(name.clone(), rest_cfg);
            price_fetchers.insert(name.clone(), Arc::new(f));
        }

        let hyperliquid = cfg
            .venues
            .hyperliquid
            .as_ref()
            .map(HyperliquidFetcher::from_sync_config);
        Ok(Self {
            onchain,
            price_fetchers,
            registry: Some(RegistryFetcher::new()),
            hyperliquid,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        })
    }

    /// 주어진 `OracleFeed` source 에 매핑되는 `PriceFetcher` 를 반환.
    fn price_fetcher_for(
        &self,
        source: &simulation_state::DataSource,
    ) -> Option<&Arc<dyn PriceFetcher>> {
        match source {
            simulation_state::DataSource::OracleFeed { provider, .. } => {
                self.price_fetchers.get(&provider_key(provider))
            }
            _ => None,
        }
    }

    /// `state` 안의 모든 stale `LiveField` 를 `now` 기준으로 갱신.
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

    /// Hyperliquid L1 계정 전체를 venue snapshot 기준으로 갱신.
    ///
    /// 이 경로는 reducer가 기록한 pending intent보다 venue state를 우선한다. 따라서
    /// 주문이 체결되거나 취소되면 다음 snapshot에서 `open_orders`, `positions`,
    /// `perp_usdc`, `agents`가 Hyperliquid 응답 기준으로 교체된다.
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

    /// `action` 안의 모든 stale `LiveField` 를 갱신. wallet refresh 와 같은 인프라
    /// (walker → batcher → fetcher) 를 재사용하되 walker 와 apply 만 Action 측.
    ///
    /// `state` 는 read-only context — fetcher 가 wallet 정보 (address 등) 가 필요할 때 참고.
    pub async fn refresh_action(
        &self,
        action: &mut simulation_reducer::action::Action,
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
        action: &mut simulation_reducer::action::Action,
        state: &WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        // 같은 fetcher 들을 호출하되, 결과를 apply_value_to_action 으로 적용.
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
                // Action live_inputs 의 OnchainView 는 인자가 필요한 경우가 많음
                // (balanceOf(user), getReserveData(asset) 등). slot 별 resolver
                // (args_resolver::resolve_args) 가 action context 에서 인자를 추출.
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
                // OnchainView 의 args 가 없는 함수만 우선 지원 (totalSupply 등).
                // 인자가 필요한 함수는 후속 phase 에서 source 메타에 args 인코드 추가.
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
                            // Registry 결과는 LiveField value 로 그대로 사용 가능한
                            // location 이 아직 없음 (TokenKind 분류는 별도 영역).
                            // 일단 token price 위치만 처리, 나머지는 후속.
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Derived => {
                // 한 batch 안의 derived 필드들끼리는 서로 독립이라고 가정 (단순한
                // 1-level 처리). 다중 layer DerivedFrom 은 호출자가 여러 번 refresh.
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    if let simulation_state::DataSource::DerivedFrom { calc_id, inputs } =
                        &item.source
                    {
                        // ★ FieldRef inputs 를 현재 state 값으로 resolve (Phase 7 완성)
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
                // 지금은 Hyperliquid 만 지원 — endpoint 가 hyperliquid 면 dispatch.
                // 향후 GMX/dYdX 추가 시 endpoint 패턴 매칭으로 분기.
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

/// 갱신된 `value` 를 state 의 해당 `LiveField.value/synced_at` 으로 반영.
///
/// 실패해도 상위에서 errors 에 누적할 뿐. state 자체는 일관성 유지.
fn apply_value(state: &mut WalletState, loc: &FieldLocation, value: Value, now: Time) {
    match loc {
        FieldLocation::TokenPrice { token_key_json } => {
            if let Ok(key) = serde_json::from_str::<simulation_state::TokenKey>(token_key_json) {
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
                if let Some(p) = value_to_optional_price(&value) {
                    field.value = p;
                    field.synced_at = now;
                    field.confidence = Some(Confidence::fresh());
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
        // Action 측 슬롯은 apply_value_to_action 이 별도로 처리.
        // 여기서는 무시 (refresh_action 흐름에서 dispatch 됨).
        FieldLocation::Action { .. } => {}
    }
}

fn value_to_price(v: &Value) -> Option<Price> {
    match v {
        Value::String(s) => Some(simulation_state::Decimal::new(s.clone())),
        Value::Number(n) => Some(simulation_state::Decimal::new(n.to_string())),
        _ => None,
    }
}

fn value_to_optional_price(v: &Value) -> Option<Option<Price>> {
    match v {
        Value::Null => Some(None),
        _ => value_to_price(v).map(Some),
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

fn set_decimal(field: &mut LiveField<simulation_state::Decimal>, v: &Value, now: Time) {
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
) -> Option<&'a mut LiveField<simulation_state::Decimal>> {
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
) -> Option<&'a mut simulation_state::PerpPosition> {
    let pos = state.positions.iter_mut().find(|p| p.id == position_id)?;
    match &mut pos.kind {
        PositionKind::PerpPosition(p) => Some(p),
        _ => None,
    }
}

const HL_ACCOUNT_ID: &str = "hyperliquid/account";

fn upsert_hyperliquid_account(
    state: &mut WalletState,
    account: simulation_state::HlAccount,
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
    use simulation_state::{Address, ChainId};

    // 실제 RPC 가 필요 없는 빈 state 의 refresh — no-op 동작 확인.
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

        let mut state = WalletState::new(simulation_state::WalletId::new(
            Address::ZERO,
            [ChainId::ethereum_mainnet()],
        ));
        let report = orch.refresh(&mut state, Time::from_unix(0)).await.unwrap();
        assert_eq!(report.walked.total_live_fields, 0);
        assert_eq!(report.fields_updated, 0);
        assert_eq!(report.batches_processed, 0);
    }

    /// `DerivedFrom` HF 가 Global `FieldRef` inputs 로부터 실제 계산되는지 end-to-end.
    /// RPC 호출 없음 — Derived batch 만 처리.
    #[tokio::test]
    async fn derived_hf_computes_from_globals() {
        use simulation_state::{
            DataSource, Decimal, Duration, FieldRef, LendingAccount, LiveField, MarketRef,
            Position, PositionKind, Time as T, VenueRef, WalletId,
        };

        // RPC 안 쓰는 orchestrator (onchain fetcher 는 존재하지만 derived 만 처리)
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

        // HF LiveField 의 source = DerivedFrom(aave_hf, [collateral, debt, liq_threshold])
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

        // stale 하도록 ttl=60, synced_at=0, now=10000
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
            protocol: simulation_state::ProtocolRef::new("aave_v3"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::LendingAccount(lending),
            primitives_synced_at: now,
            primitives_source: DataSource::UserSupplied,
        });

        let report = orch.refresh(&mut state, now).await.unwrap();
        assert!(report.fields_updated >= 1, "HF should have been updated");

        // HF 가 1.6 으로 계산됐는지 확인
        if let PositionKind::LendingAccount(la) = &state.positions[0].kind {
            assert_eq!(la.health_factor.value.as_str(), "1.6");
        } else {
            panic!("expected lending account");
        }
    }

    #[tokio::test]
    async fn sync_hyperliquid_account_replaces_local_account_with_snapshot() {
        use std::str::FromStr;

        use serde_json::{json, Value};
        use simulation_state::{
            DataSource, Decimal, HlAccount, HlOpenOrder, Position, PositionKind, ProtocolRef,
            Time as T, WalletId,
        };
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
                                "time": 1710000000123u64
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
                                "time": 1710000000123u64
                            }),
                            ("frontendOpenOrders", Some("xyz")) => json!([{
                                "timestamp": 1780211477428u64,
                                "coin": "xyz:SPCX",
                                "side": "A",
                                "limitPx": "170.2",
                                "sz": "0.0",
                                "oid": 449792035550u64,
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
                                "timestamp": 1710000000124u64,
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
                                "validUntil": 1710000000999u64
                            }]),
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
            simulation_state::WalletState::new(WalletId::new(user, [ChainId::ethereum_mainnet()]));
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
                leverage_settings: Vec::new(),
                agents: Vec::new(),
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
        assert_eq!(account.agents.len(), 1);
    }
}
