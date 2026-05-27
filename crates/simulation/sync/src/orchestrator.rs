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
use crate::fetchers::{ChainlinkFetcher, OnchainViewFetcher, RegistryFetcher};
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
    calc: CalcRegistry,
    // 향후 추가:
    // venue: VenueRegistry,
}

impl Orchestrator {
    pub fn new(onchain: OnchainViewFetcher) -> Self {
        Self {
            onchain,
            chainlink: None,
            registry: None,
            calc: CalcRegistry::with_builtins(),
        }
    }

    pub fn with_chainlink(mut self, chainlink: ChainlinkFetcher) -> Self {
        self.chainlink = Some(chainlink);
        self
    }

    pub fn with_registry(mut self, registry: RegistryFetcher) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_calc(mut self, calc: CalcRegistry) -> Self {
        self.calc = calc;
        self
    }

    pub fn from_rpc_router(router: Arc<crate::RpcRouter>) -> Self {
        let onchain = OnchainViewFetcher::new(router.clone());
        let chainlink = ChainlinkFetcher::new(router);
        Self {
            onchain,
            chainlink: Some(chainlink),
            registry: Some(RegistryFetcher::new()),
            calc: CalcRegistry::with_builtins(),
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
                    if let simulation_state::DataSource::DerivedFrom {
                        calc_id,
                        inputs: _,
                    } = &item.source
                    {
                        let ctx = CalcContext {
                            state,
                            inputs: vec![], // input resolver 는 후속 — 지금은 빈 입력으로 stub
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

            // Phase 8 에서 추가
            BatchKind::Venue { .. } | BatchKind::UserSupplied => Ok((0, 0)),
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
}
