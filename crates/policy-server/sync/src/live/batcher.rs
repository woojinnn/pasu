use std::collections::BTreeMap;

use policy_state::{ChainId, DataSource};

use crate::walker::StaleField;

#[derive(Debug)]
pub struct FetchBatch {
    pub kind: BatchKind,
    pub items: Vec<StaleField>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchKind {
    Onchain { chain: ChainId },
    Oracle,
    Venue { endpoint: String },
    Registry { endpoint: String },
    Derived,
    UserSupplied,
}

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
            DataSource::UserSupplied => {}
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
    use policy_state::{Address, ChainId, DataSource, OracleProvider, Time};

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
