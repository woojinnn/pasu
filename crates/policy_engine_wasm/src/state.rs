//! Per-WASM-instance state and snapshot-backed host capabilities.

use crate::dto::{OracleEntryDto, WindowEntryDto};
use alloy_primitives::U256;
use policy_engine::core::{Address, AmountSpec, Token, UsdValuation};
use policy_engine::host::approvals::{Approvals, ApprovalsError};
use policy_engine::host::clock::Clock;
use policy_engine::host::oracle::SnapshotOracle;
use policy_engine::host::portfolio::{Portfolio, PortfolioError};
use policy_engine::host::stat_windows::{
    ReservationId, StatDelta, StatKey, StatValue, StatWindows,
};
use policy_engine::policy::PolicyEngine;
use policy_engine_adapters_bundle::{default_registry, default_signature_registry};
use std::cell::RefCell;
use std::collections::HashMap;

pub struct EngineState {
    pub policies: PolicyEngine,
}

thread_local! {
    pub static STATE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

pub fn registry() -> impl policy_engine::registry::AdapterRegistry {
    default_registry()
}

pub fn signature_registry() -> impl policy_engine::registry::SignatureRegistry {
    default_signature_registry()
}

pub fn snapshot_oracle_from_entries(entries: &[OracleEntryDto]) -> SnapshotOracle {
    let mut oracle = SnapshotOracle::new();
    for entry in entries {
        let Some((chain_id, address)) = token_key_parts(&entry.token_key) else {
            continue;
        };
        let token = Token {
            chain_id,
            address,
            symbol: String::new(),
            decimals: 0,
            is_native: false,
        };
        oracle.insert(
            &token,
            UsdValuation {
                value: entry.usd_per_unit.clone(),
                as_of_ts: entry.as_of_ts,
                sources: entry.sources.clone(),
                stale_sec: entry.stale_sec,
            },
        );
    }
    oracle
}

fn token_key_parts(token_key: &str) -> Option<(u64, Address)> {
    let (chain_id, address) = token_key.split_once(':')?;
    let chain_id = chain_id.parse().ok()?;
    let address = Address::new(address).ok()?;
    Some((chain_id, address))
}

#[derive(Debug, Default)]
pub struct SnapshotPortfolio {
    balances: HashMap<(String, String), String>,
}

impl SnapshotPortfolio {
    pub fn from_entries(entries: &[crate::dto::BalanceEntryDto]) -> Self {
        Self {
            balances: entries
                .iter()
                .map(|entry| {
                    (
                        (entry.owner.to_lowercase(), entry.token_key.clone()),
                        entry.balance.clone(),
                    )
                })
                .collect(),
        }
    }
}

impl Portfolio for SnapshotPortfolio {
    fn balance(&self, owner: &Address, token: &Token) -> Result<AmountSpec, PortfolioError> {
        let key = (owner.as_str().to_lowercase(), token.key());
        let raw = self
            .balances
            .get(&key)
            .ok_or_else(|| PortfolioError::NoRecord {
                owner: owner.as_str().to_string(),
                token: token.key(),
            })?;
        Ok(amount_from_raw_string(token, raw))
    }
}

#[derive(Debug, Default)]
pub struct SnapshotApprovals {
    allowances: HashMap<(String, String, String), String>,
}

impl SnapshotApprovals {
    pub fn from_entries(entries: &[crate::dto::AllowanceEntryDto]) -> Self {
        Self {
            allowances: entries
                .iter()
                .map(|entry| {
                    (
                        (
                            entry.owner.to_lowercase(),
                            entry.token_key.clone(),
                            entry.spender.to_lowercase(),
                        ),
                        entry.allowance.clone(),
                    )
                })
                .collect(),
        }
    }
}

impl Approvals for SnapshotApprovals {
    fn allowance(
        &self,
        owner: &Address,
        token: &Token,
        spender: &Address,
    ) -> Result<AmountSpec, ApprovalsError> {
        let key = (
            owner.as_str().to_lowercase(),
            token.key(),
            spender.as_str().to_lowercase(),
        );
        let raw = self
            .allowances
            .get(&key)
            .ok_or_else(|| ApprovalsError::NoRecord {
                owner: owner.as_str().to_string(),
                token: token.key(),
                spender: spender.as_str().to_string(),
            })?;
        Ok(amount_from_raw_string(token, raw))
    }
}

fn amount_from_raw_string(token: &Token, raw: &str) -> AmountSpec {
    if let Ok(value) = U256::from_str_radix(raw, 10) {
        return AmountSpec::from_raw(token.clone(), value);
    }
    AmountSpec {
        token: token.clone(),
        raw: raw.to_string(),
        human: None,
        usd: None,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Default)]
pub struct SnapshotStatWindows {
    windows: HashMap<(String, StatKey), StatValue>,
}

impl SnapshotStatWindows {
    pub fn from_entries(entries: &[WindowEntryDto]) -> Self {
        let mut windows = HashMap::new();
        for entry in entries {
            let Some((key, value)) = stat_value_from_entry(entry) else {
                continue;
            };
            windows.insert((entry.actor.to_lowercase(), key), value);
        }
        Self { windows }
    }
}

impl StatWindows for SnapshotStatWindows {
    fn snapshot(&self, owner: &Address, keys: &[StatKey]) -> HashMap<StatKey, StatValue> {
        let owner = owner.as_str().to_lowercase();
        keys.iter()
            .filter_map(|key| {
                self.windows
                    .get(&(owner.clone(), *key))
                    .cloned()
                    .map(|value| (*key, value))
            })
            .collect()
    }

    fn reserve(&self, _owner: &Address, _deltas: Vec<StatDelta>) -> ReservationId {
        ReservationId(0)
    }

    fn settle(&self, _id: ReservationId) {}

    fn release(&self, _id: ReservationId) {}
}

fn stat_value_from_entry(entry: &WindowEntryDto) -> Option<(StatKey, StatValue)> {
    if entry.name == StatKey::SWAP_VOLUME_USD_24H.as_str() {
        Some((
            StatKey::SWAP_VOLUME_USD_24H,
            StatValue::Decimal(entry.value.clone()),
        ))
    } else if entry.name == StatKey::SWAP_COUNT_24H.as_str() {
        entry
            .value
            .parse()
            .ok()
            .map(|count| (StatKey::SWAP_COUNT_24H, StatValue::Count(count)))
    } else {
        None
    }
}
