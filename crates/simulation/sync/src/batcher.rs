//! Batcher — walker 결과 [`StaleField`] 들을 source 종류별로 묶는다.
//!
//! 같은 source kind (OnchainView/OracleFeed/VenueApi/RegistryApi) 끼리는 한 fetcher
//! 가 처리하고, `OnchainView` 안에서도 같은 chain 이면 multicall 로 더 묶을 수 있다.
//! `DerivedFrom` 은 별도 — 다른 `LiveField` 값에 의존하므로 위상정렬 후 처리.

use std::collections::BTreeMap;

use simulation_state::{ChainId, DataSource};

use crate::walker::StaleField;

/// 같은 fetcher 가 한 번에 처리할 수 있는 묶음.
#[derive(Debug)]
pub struct FetchBatch {
    pub kind: BatchKind,
    pub items: Vec<StaleField>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchKind {
    /// 같은 chain 의 `OnchainView` 들 — Multicall3 가능.
    Onchain { chain: ChainId },
    /// `OracleFeed` 들 — provider 별로 batch 효율 다름. 일단 한 묶음.
    Oracle,
    /// `VenueApi` — endpoint 별로 분리 (HTTP host 단위).
    Venue { endpoint: String },
    /// `RegistryApi` — endpoint 별로 분리.
    Registry { endpoint: String },
    /// 다른 `LiveField` 값에서 도출. 위상정렬 후 calc 적용.
    Derived,
    /// 사용자 입력 — sync 안 함.
    UserSupplied,
}

/// stale 목록 → batch 목록.
#[must_use]
pub fn batch_by_source(stale: Vec<StaleField>) -> Vec<FetchBatch> {
    let mut onchain_by_chain: BTreeMap<ChainId, Vec<StaleField>> = BTreeMap::new();
    let mut oracle: Vec<StaleField> = Vec::new();
    let mut venue_by_endpoint: BTreeMap<String, Vec<StaleField>> = BTreeMap::new();
    let mut registry_by_endpoint: BTreeMap<String, Vec<StaleField>> = BTreeMap::new();
    let mut derived: Vec<StaleField> = Vec::new();

    for f in stale {
        match &f.source {
            DataSource::OnchainView { chain, .. } => {
                onchain_by_chain.entry(chain.clone()).or_default().push(f);
            }
            DataSource::OracleFeed { .. } => oracle.push(f),
            DataSource::VenueApi { endpoint, .. } => {
                venue_by_endpoint
                    .entry(endpoint.clone())
                    .or_default()
                    .push(f);
            }
            DataSource::RegistryApi { endpoint, .. } => {
                registry_by_endpoint
                    .entry(endpoint.clone())
                    .or_default()
                    .push(f);
            }
            DataSource::DerivedFrom { .. } => derived.push(f),
            DataSource::UserSupplied => {
                // 무시 — orchestrator 가 skip.
            }
        }
    }

    let mut batches = Vec::new();

    for (chain, items) in onchain_by_chain {
        batches.push(FetchBatch {
            kind: BatchKind::Onchain { chain },
            items,
        });
    }
    if !oracle.is_empty() {
        batches.push(FetchBatch {
            kind: BatchKind::Oracle,
            items: oracle,
        });
    }
    for (endpoint, items) in venue_by_endpoint {
        batches.push(FetchBatch {
            kind: BatchKind::Venue { endpoint },
            items,
        });
    }
    for (endpoint, items) in registry_by_endpoint {
        batches.push(FetchBatch {
            kind: BatchKind::Registry { endpoint },
            items,
        });
    }
    if !derived.is_empty() {
        batches.push(FetchBatch {
            kind: BatchKind::Derived,
            items: derived,
        });
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::{Address, ChainId, DataSource, OracleProvider, Time};

    use crate::walker::{FieldLocation, StaleField};

    fn mk_stale(source: DataSource) -> StaleField {
        StaleField {
            location: FieldLocation::TokenPrice {
                token_key_json: "x".into(),
            },
            source,
            synced_at: Time::from_unix(0),
        }
    }

    #[test]
    fn groups_onchain_by_chain() {
        let stale = vec![
            mk_stale(DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: Address::ZERO,
                function: "balanceOf(address)".into(),
                decoder_id: "u256".into(),
            }),
            mk_stale(DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: Address::ZERO,
                function: "totalSupply()".into(),
                decoder_id: "u256".into(),
            }),
            mk_stale(DataSource::OnchainView {
                chain: ChainId::arbitrum(),
                contract: Address::ZERO,
                function: "balanceOf(address)".into(),
                decoder_id: "u256".into(),
            }),
        ];

        let batches = batch_by_source(stale);
        assert_eq!(batches.len(), 2);
        // BTreeMap 정렬 → arbitrum first (eip155:42161 < eip155:1? 문자열 정렬이라
        // "eip155:1" < "eip155:42161" 따라서 mainnet first 가 맞음)
        let mainnet_batch = batches
            .iter()
            .find(|b| {
                matches!(&b.kind, BatchKind::Onchain { chain }
                    if chain.as_str() == "eip155:1")
            })
            .unwrap();
        assert_eq!(mainnet_batch.items.len(), 2);
    }

    #[test]
    fn groups_oracle_together() {
        let stale = vec![
            mk_stale(DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "USDC/USD".into(),
            }),
            mk_stale(DataSource::OracleFeed {
                provider: OracleProvider::Pyth,
                feed_id: "ETH/USD".into(),
            }),
        ];
        let batches = batch_by_source(stale);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].kind, BatchKind::Oracle);
        assert_eq!(batches[0].items.len(), 2);
    }

    #[test]
    fn user_supplied_skipped() {
        let stale = vec![mk_stale(DataSource::UserSupplied)];
        let batches = batch_by_source(stale);
        assert_eq!(batches.len(), 0);
    }
}
