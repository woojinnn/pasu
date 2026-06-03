//! Bridge a completed sync tick to real-time `wallet_synced` events.
//!
//! The on-demand `POST /wallets/:addr/sync` path already emits a `wallet_synced`
//! event when it finishes. The background `sync_worker` runs in a separate
//! process and used to stay silent, so dashboards never saw its refreshes live.
//! This helper turns the wallets a tick refreshed into one event each, published
//! through the same `EventPublisher` boundary (Redis in cloud) so connected
//! dashboards update as the worker runs.

use policy_sync::WalletSyncCounts;

use crate::events::{Event, EventPublisher, WalletSync};

/// Publish one `wallet_synced` event per wallet that a sync tick refreshed.
///
/// `synced_at` is a Unix timestamp in seconds, passed in so the caller owns the
/// clock and tests stay deterministic. Each wallet's real refresh counts ride
/// along in the payload.
pub async fn publish_tick_events(
    publisher: &dyn EventPublisher,
    user_id: &str,
    synced_wallets: &[WalletSyncCounts],
    synced_at: i64,
) {
    for w in synced_wallets {
        publisher
            .publish(
                user_id.to_owned(),
                Event::WalletSynced(WalletSync {
                    wallet: format!("{:#x}", w.wallet.address),
                    fields_updated: w.fields_updated,
                    fields_failed: w.fields_failed,
                    synced_at,
                }),
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use policy_state::{Address, ChainId, WalletId};

    #[derive(Default)]
    struct RecordingPublisher {
        published: Mutex<Vec<(String, Event)>>,
    }

    #[async_trait]
    impl EventPublisher for RecordingPublisher {
        async fn publish(&self, user_id: String, event: Event) {
            self.published.lock().unwrap().push((user_id, event));
        }
    }

    fn counts(addr: Address, updated: usize, failed: usize) -> WalletSyncCounts {
        WalletSyncCounts {
            wallet: WalletId::new(addr, [ChainId::ethereum_mainnet()]),
            fields_updated: updated,
            fields_failed: failed,
        }
    }

    #[tokio::test]
    async fn publishes_one_wallet_synced_event_per_wallet_with_counts() {
        let publisher = RecordingPublisher::default();
        let wallets = [counts(Address::ZERO, 3, 1)];

        publish_tick_events(&publisher, "u_alice", &wallets, 1_700_000_000).await;

        let recorded = publisher.published.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        let (user_id, event) = &recorded[0];
        assert_eq!(user_id, "u_alice");
        match event {
            Event::WalletSynced(sync) => {
                assert_eq!(sync.wallet, format!("{:#x}", Address::ZERO));
                assert_eq!(sync.fields_updated, 3);
                assert_eq!(sync.fields_failed, 1);
                assert_eq!(sync.synced_at, 1_700_000_000);
            }
            other => panic!("expected WalletSynced, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn no_wallets_publishes_nothing() {
        let publisher = RecordingPublisher::default();
        publish_tick_events(&publisher, "u_alice", &[], 1).await;
        assert!(publisher.published.lock().unwrap().is_empty());
    }
}
