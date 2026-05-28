//! Orchestrator — walker + batcher + fetchers 를 묶어 한 `WalletState` 를 신선화.
//!
//! 일반 흐름:
//! 1. [`walk_stale`] 로 stale LiveField 수집
//! 2. [`batch_by_source`] 로 source 별 묶음
//! 3. 각 batch 를 해당 fetcher 로 dispatch
//! 4. 결과 (`Value`) 를 다시 state 의 LiveField 에 write back
//!
//! Phase 4 에선 OnchainView 만 wired up. 나머지 (Oracle/Venue/Registry/Derived) 는
//! 후속 phase 에서 차례로.

use std::sync::Arc;

use serde_json::Value;

use simulation_state::{Confidence, LiveField, PositionKind, Price, Time, WalletState};

use crate::batcher::{BatchKind, FetchBatch, batch_by_source};
use crate::error::SyncError;
use crate::fetchers::onchain::OnchainCall;
use crate::calc::{CalcContext, CalcRegistry};
use crate::fetchers::{ChainlinkFetcher, HyperliquidFetcher, OnchainViewFetcher, RegistryFetcher};
use crate::walker::{FieldLocation, WalkStats, walk_stale};

/// 한 wallet refresh 결과 요약 — 디버깅 / 메트릭용.
#[derive(Debug, Default, Clone)]
pub struct RefreshReport {
    pub walked: WalkStats,
    pub batches_processed: usize,
    pub fields_updated: usize,
    pub fields_failed: usize,
    pub errors: Vec<String>,
}

pub struct Orchestrator {
    onchain: OnchainViewFetcher,
    chainlink: Option<ChainlinkFetcher>,
    registry: Option<RegistryFetcher>,
    hyperliquid: Option<HyperliquidFetcher>,
    calc: CalcRegistry,
    /// Global LiveField 값 (gas_price, eth_usd 등). DerivedFrom 의 Global FieldRef
    /// resolve 에 사용. scheduler/sync 가 주기적으로 갱신.
    globals: crate::resolver::GlobalValues,
    /// primitives sync (balance/block_height/approval) 용 직접 router 접근.
    router: Option<Arc<crate::RpcRouter>>,
}

impl Orchestrator {
    pub fn new(onchain: OnchainViewFetcher) -> Self {
        Self {
            onchain,
            chainlink: None,
            registry: None,
            hyperliquid: None,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: None,
        }
    }

    /// Global LiveField 값 갱신 (gas_price, eth_usd 등).
    pub fn set_global(&mut self, name: impl Into<String>, value: serde_json::Value) {
        self.globals.insert(name.into(), value);
    }

    /// primitives sync 가 사용할 router 참조.
    pub(crate) fn router_ref(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    pub fn with_chainlink(mut self, chainlink: ChainlinkFetcher) -> Self {
        self.chainlink = Some(chainlink);
        self
    }

    pub fn with_registry(mut self, registry: RegistryFetcher) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_hyperliquid(mut self, hl: HyperliquidFetcher) -> Self {
        self.hyperliquid = Some(hl);
        self
    }

    pub fn with_calc(mut self, calc: CalcRegistry) -> Self {
        self.calc = calc;
        self
    }

    pub fn from_rpc_router(router: Arc<crate::RpcRouter>) -> Self {
        let onchain = OnchainViewFetcher::new(router.clone());
        let chainlink = ChainlinkFetcher::new(router.clone());
        Self {
            onchain,
            chainlink: Some(chainlink),
            registry: Some(RegistryFetcher::new()),
            hyperliquid: Some(HyperliquidFetcher::new()),
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        }
    }

    /// `state` 안의 모든 stale LiveField 를 `now` 기준으로 갱신.
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
                    report.errors.push(format!("{}", e));
                }
            }
        }
        Ok(report)
    }

    /// `action` 안의 모든 stale LiveField 를 갱신. wallet refresh 와 같은 인프라
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
            match self.process_batch_for_action(batch, action, state, now).await {
                Ok((ok, fail)) => {
                    report.fields_updated += ok;
                    report.fields_failed += fail;
                }
                Err(e) => {
                    report.errors.push(format!("{}", e));
                }
            }
        }
        Ok(report)
    }

    async fn process_batch_for_action(
        &self,
        batch: FetchBatch,
        action: &mut simulation_reducer::action::Action,
        _state: &WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        // 같은 fetcher 들을 호출하되, 결과를 apply_value_to_action 으로 적용.
        let mut ok = 0usize;
        let mut fail = 0usize;
        match &batch.kind {
            BatchKind::Oracle => {
                let Some(cl) = self.chainlink.as_ref() else {
                    return Ok((0, batch.items.len()));
                };
                for item in batch.items {
                    match cl.fetch_price(&item.source).await {
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
                // Action live_inputs 의 OnchainView 는 인자가 필요한 경우가 많아
                // (balanceOf(user), getReserveData(asset) 등). Phase 1 에서는
                // 인자 없는 케이스만 처리하거나 호출자가 source.function 안에 args 인코드.
                let calls: Result<Vec<_>, _> = batch
                    .items
                    .iter()
                    .map(|item| {
                        crate::fetchers::onchain::OnchainCall::from_source(&item.source, vec![])
                    })
                    .collect();
                let Ok(calls) = calls else {
                    return Ok((0, batch.items.len()));
                };
                let outcomes = self.onchain.fetch_batch(chain, &calls).await?;
                for (item, outcome) in batch.items.into_iter().zip(outcomes.into_iter()) {
                    if outcome.success {
                        if let Some(value) = outcome.value {
                            crate::action_walk::apply_value_to_action(action, &item.location, value, now);
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
                            crate::action_walk::apply_value_to_action(action, &item.location, v, now);
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Venue { endpoint } => {
                let is_hl = endpoint.contains("hyperliquid");
                let Some(hl) = (if is_hl { self.hyperliquid.as_ref() } else { None }) else {
                    return Ok((0, batch.items.len()));
                };
                for item in batch.items {
                    match hl.fetch(&item.source).await {
                        Ok(v) => {
                            crate::action_walk::apply_value_to_action(action, &item.location, v, now);
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
                let chainlink = match self.chainlink.as_ref() {
                    Some(c) => c,
                    None => return Ok((0, batch.items.len())),
                };

                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    match chainlink.fetch_price(&item.source).await {
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
                let is_hl = endpoint.contains("hyperliquid")
                    || endpoint == "https://api.hyperliquid.xyz/info";
                let hl = if is_hl { self.hyperliquid.as_ref() } else { None };
                let hl = match hl {
                    Some(h) => h,
                    None => return Ok((0, batch.items.len())),
                };
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    match hl.fetch(&item.source).await {
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

/// 갱신된 `value` 를 state 의 해당 LiveField.value/synced_at 으로 반영.
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
            if let Some(price) = perp_field_mut(state, position_id, PerpMetric::Mark) {
                if let Some(p) = value_to_price(&value) {
                    price.value = p;
                    price.synced_at = now;
                    price.confidence = Some(Confidence::fresh());
                }
            }
        }
        // 나머지 perp 필드는 후속 phase 에서 dispatch 추가
        FieldLocation::PerpLiqPrice { .. }
        | FieldLocation::PerpUnrealizedPnl { .. }
        | FieldLocation::PerpFundingOwed { .. }
        | FieldLocation::PerpLeverage { .. } => {
            // TODO Phase 8: venue fetcher 결과 처리
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

enum PerpMetric {
    Mark,
}

fn perp_field_mut<'a>(
    state: &'a mut WalletState,
    position_id: &str,
    metric: PerpMetric,
) -> Option<&'a mut LiveField<Price>> {
    let pos = state.positions.iter_mut().find(|p| p.id == position_id)?;
    match &mut pos.kind {
        PositionKind::PerpPosition(p) => match metric {
            PerpMetric::Mark => Some(&mut p.mark_price),
        },
        _ => None,
    }
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

    /// DerivedFrom HF 가 Global FieldRef inputs 로부터 실제 계산되는지 end-to-end.
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

        let mut state = WalletState::new(WalletId::new(
            Address::ZERO,
            [ChainId::ethereum_mainnet()],
        ));
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
}
