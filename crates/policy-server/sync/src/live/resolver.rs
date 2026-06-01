//! `FieldRef` resolver — `DataSource::DerivedFrom.inputs` 의 `FieldRef` 들을
//! 현재 state 의 실제 Value 로 변환한다.
//!
//! Phase 7 의 빠진 고리: orchestrator 가 calc 를 호출할 때 inputs 를 채워야 하는데,
//! 그 inputs 는 다른 `LiveField` (또는 직접 필드) 의 현재 값이다. 이 모듈이 그 lookup
//! 을 담당.
//!
//! Global `FieldRef` (`gas_price`, `eth_usd`) 는 wallet state 안에 없으므로, 호출자가
//! 별도 `globals` map 으로 주입한다 (`global_live_fields` 테이블의 in-memory view).

use std::collections::HashMap;

use serde_json::Value;

use simulation_state::{FieldRef, PositionFieldName, PositionKind, TokenFieldName, WalletState};

/// Global `LiveField` 값들의 in-memory view (`gas_price`, `eth_usd` 등).
pub type GlobalValues = HashMap<String, Value>;

/// 한 `FieldRef` 를 현재 값으로 resolve. 없으면 None.
#[must_use]
pub fn resolve_field(
    state: &WalletState,
    globals: &GlobalValues,
    field_ref: &FieldRef,
) -> Option<Value> {
    match field_ref {
        FieldRef::TokenField {
            token_key_json,
            field,
        } => {
            let key = serde_json::from_str::<simulation_state::TokenKey>(token_key_json).ok()?;
            let holding = state.tokens.get(&key)?;
            match field {
                TokenFieldName::PriceUsd => holding
                    .price_usd
                    .as_ref()
                    .map(|p| Value::String(p.value.0.clone())),
            }
        }

        FieldRef::PositionField { position_id, field } => {
            let pos = state.positions.iter().find(|p| &p.id == position_id)?;
            resolve_position_field(&pos.kind, field)
        }

        FieldRef::PendingField {
            pending_id,
            field: _,
        } => {
            // pending lifecycle 값 — 현재 단순히 status 문자열만.
            let pending = state.pending.iter().find(|p| &p.id == pending_id)?;
            Some(Value::String(format!("{:?}", pending.lifecycle.status)))
        }

        FieldRef::Global { name } => globals.get(name).cloned(),
    }
}

fn resolve_position_field(kind: &PositionKind, field: &PositionFieldName) -> Option<Value> {
    match (kind, field) {
        (PositionKind::LendingAccount(la), PositionFieldName::HealthFactor) => {
            Some(Value::String(la.health_factor.value.0.clone()))
        }
        (PositionKind::LendingAccount(la), PositionFieldName::Ltv) => {
            Some(Value::String(la.ltv.value.0.clone()))
        }
        (PositionKind::LendingAccount(la), PositionFieldName::LiquidationThreshold) => {
            Some(Value::String(la.liquidation_threshold.value.0.clone()))
        }
        (PositionKind::PerpPosition(p), PositionFieldName::MarkPrice) => {
            Some(Value::String(p.mark_price.value.0.clone()))
        }
        (PositionKind::PerpPosition(p), PositionFieldName::LiqPrice) => p
            .liq_price
            .value
            .as_ref()
            .map(|d| Value::String(d.0.clone())),
        (PositionKind::PerpPosition(p), PositionFieldName::UnrealizedPnl) => {
            Some(Value::String(p.unrealized_pnl.value.to_string()))
        }
        (PositionKind::PerpPosition(p), PositionFieldName::FundingOwed) => {
            Some(Value::String(p.funding_owed.value.to_string()))
        }
        (PositionKind::PerpPosition(p), PositionFieldName::Leverage) => {
            Some(Value::String(p.leverage.value.0.clone()))
        }
        _ => None,
    }
}

/// `inputs` 들을 순서대로 resolve. 못 찾은 input 은 Null 로 (calc 가 판단).
#[must_use]
pub fn resolve_inputs(
    state: &WalletState,
    globals: &GlobalValues,
    inputs: &[FieldRef],
) -> Vec<Value> {
    inputs
        .iter()
        .map(|fr| resolve_field(state, globals, fr).unwrap_or(Value::Null))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::{
        Address, ChainId, DataSource, Decimal, LiveField, OracleProvider, Time, TokenFieldName,
        TokenKey, WalletId, WalletState,
    };
    use std::str::FromStr;

    fn state_with_usdc_price() -> (WalletState, String) {
        use simulation_state::{
            Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKind,
        };
        let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdc,
        };
        let key_json = serde_json::to_string(&key).unwrap();
        let holding = TokenHolding {
            key: key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(LiveField::new(
                Decimal::new("1.0001"),
                DataSource::OracleFeed {
                    provider: OracleProvider::Chainlink,
                    feed_id: "USDC/USD".into(),
                },
                Time::from_unix(1_738_000_000),
            )),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_738_000_000),
            primitives_source: DataSource::UserSupplied,
        };
        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        state.tokens.insert(key, holding);
        (state, key_json)
    }

    #[test]
    fn resolves_token_price() {
        let (state, key_json) = state_with_usdc_price();
        let globals = GlobalValues::new();
        let fr = FieldRef::TokenField {
            token_key_json: key_json,
            field: TokenFieldName::PriceUsd,
        };
        let v = resolve_field(&state, &globals, &fr).unwrap();
        assert_eq!(v, Value::String("1.0001".into()));
    }

    #[test]
    fn resolves_global() {
        let (state, _) = state_with_usdc_price();
        let mut globals = GlobalValues::new();
        globals.insert("gas_price".into(), Value::String("25000000000".into()));
        let fr = FieldRef::Global {
            name: "gas_price".into(),
        };
        let v = resolve_field(&state, &globals, &fr).unwrap();
        assert_eq!(v, Value::String("25000000000".into()));
    }

    #[test]
    fn missing_field_resolves_to_null() {
        let (state, _) = state_with_usdc_price();
        let globals = GlobalValues::new();
        let fr = FieldRef::Global {
            name: "nonexistent".into(),
        };
        let inputs = resolve_inputs(&state, &globals, &[fr]);
        assert_eq!(inputs, vec![Value::Null]);
    }
}
