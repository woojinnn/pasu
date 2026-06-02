//! ```ignore
//! let scope = ActionScope::from_token_keys(touched_keys);
//! orch.refresh_for_scope(&mut state, &scope, now).await?;
//! ```
//!
//! This module only derives refresh scopes from actions; action schema details
//! stay owned by the action model.
use std::collections::HashSet;

use policy_state::{PositionKind, TokenKey, WalletState};

use crate::error::SyncError;
use crate::orchestrator::{Orchestrator, RefreshReport};
use crate::walker::{FieldLocation, StaleField, WalkStats};

#[derive(Clone, Debug, Default)]
pub struct ActionScope {
    pub tokens: HashSet<TokenKey>,
    pub lending_positions: HashSet<String>,
    pub perp_positions: HashSet<String>,
}

impl ActionScope {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_token_keys(keys: impl IntoIterator<Item = TokenKey>) -> Self {
        Self {
            tokens: keys.into_iter().collect(),
            ..Default::default()
        }
    }

    pub fn touch_token(&mut self, key: TokenKey) -> &mut Self {
        self.tokens.insert(key);
        self
    }

    pub fn touch_lending(&mut self, position_id: impl Into<String>) -> &mut Self {
        self.lending_positions.insert(position_id.into());
        self
    }

    pub fn touch_perp(&mut self, position_id: impl Into<String>) -> &mut Self {
        self.perp_positions.insert(position_id.into());
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
            && self.lending_positions.is_empty()
            && self.perp_positions.is_empty()
    }
}

#[must_use]
pub fn walk_scope(state: &WalletState, scope: &ActionScope) -> (Vec<StaleField>, WalkStats) {
    let mut stale = Vec::new();
    let mut stats = WalkStats::default();

    // tokens
    for key in &scope.tokens {
        if let Some(holding) = state.tokens.get(key) {
            if let Some(price) = holding.price_usd.as_ref() {
                stats.total_live_fields += 1;
                stats.stale_count += 1;
                stale.push(StaleField {
                    location: FieldLocation::TokenPrice {
                        token_key_json: serde_json::to_string(key).unwrap_or_default(),
                    },
                    source: price.source.clone(),
                    synced_at: price.synced_at,
                });
            }
        }
    }

    // lending positions
    for pos_id in &scope.lending_positions {
        if let Some(pos) = state.positions.iter().find(|p| &p.id == pos_id) {
            if let PositionKind::LendingAccount(la) = &pos.kind {
                push_field(
                    &mut stale,
                    &mut stats,
                    &la.health_factor.source,
                    la.health_factor.synced_at,
                    FieldLocation::LendingHealthFactor {
                        position_id: pos_id.clone(),
                    },
                );
                push_field(
                    &mut stale,
                    &mut stats,
                    &la.ltv.source,
                    la.ltv.synced_at,
                    FieldLocation::LendingLtv {
                        position_id: pos_id.clone(),
                    },
                );
                push_field(
                    &mut stale,
                    &mut stats,
                    &la.liquidation_threshold.source,
                    la.liquidation_threshold.synced_at,
                    FieldLocation::LendingLiquidationThreshold {
                        position_id: pos_id.clone(),
                    },
                );
            }
        }
    }

    // perp positions
    for pos_id in &scope.perp_positions {
        if let Some(pos) = state.positions.iter().find(|p| &p.id == pos_id) {
            if let PositionKind::PerpPosition(p) = &pos.kind {
                push_field(
                    &mut stale,
                    &mut stats,
                    &p.mark_price.source,
                    p.mark_price.synced_at,
                    FieldLocation::PerpMarkPrice {
                        position_id: pos_id.clone(),
                    },
                );
                push_field(
                    &mut stale,
                    &mut stats,
                    &p.liq_price.source,
                    p.liq_price.synced_at,
                    FieldLocation::PerpLiqPrice {
                        position_id: pos_id.clone(),
                    },
                );
                push_field(
                    &mut stale,
                    &mut stats,
                    &p.unrealized_pnl.source,
                    p.unrealized_pnl.synced_at,
                    FieldLocation::PerpUnrealizedPnl {
                        position_id: pos_id.clone(),
                    },
                );
                push_field(
                    &mut stale,
                    &mut stats,
                    &p.funding_owed.source,
                    p.funding_owed.synced_at,
                    FieldLocation::PerpFundingOwed {
                        position_id: pos_id.clone(),
                    },
                );
                push_field(
                    &mut stale,
                    &mut stats,
                    &p.leverage.source,
                    p.leverage.synced_at,
                    FieldLocation::PerpLeverage {
                        position_id: pos_id.clone(),
                    },
                );
            }
        }
    }

    (stale, stats)
}

fn push_field(
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
    source: &policy_state::DataSource,
    synced_at: policy_state::Time,
    location: FieldLocation,
) {
    stats.total_live_fields += 1;
    stats.stale_count += 1;
    stale.push(StaleField {
        location,
        source: source.clone(),
        synced_at,
    });
}

impl Orchestrator {
    pub async fn refresh_for_scope(
        &self,
        state: &mut WalletState,
        scope: &ActionScope,
        now: policy_state::Time,
    ) -> Result<RefreshReport, SyncError> {
        if scope.is_empty() {
            return Ok(RefreshReport::default());
        }

        let (stale, walked) = walk_scope(state, scope);
        let mut report = RefreshReport {
            walked,
            ..Default::default()
        };
        if stale.is_empty() {
            return Ok(report);
        }

        let batches = crate::batcher::batch_by_source(stale);
        for batch in batches {
            report.batches_processed += 1;
            match self.process_batch_public(batch, state, now).await {
                Ok((ok, fail)) => {
                    report.fields_updated += ok;
                    report.fields_failed += fail;
                }
                Err(e) => report.errors.push(format!("{e}")),
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId, TokenKey};
    use std::str::FromStr;

    #[test]
    fn scope_builder() {
        let mut scope = ActionScope::new();
        scope.touch_token(TokenKey::Native {
            chain: ChainId::ethereum_mainnet(),
        });
        scope.touch_lending("aave_v3:eth".to_string());
        assert_eq!(scope.tokens.len(), 1);
        assert_eq!(scope.lending_positions.len(), 1);
        assert!(scope.perp_positions.is_empty());
        assert!(!scope.is_empty());
    }

    #[test]
    fn from_token_keys_helper() {
        let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let scope = ActionScope::from_token_keys([TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdc,
        }]);
        assert_eq!(scope.tokens.len(), 1);
    }
}
