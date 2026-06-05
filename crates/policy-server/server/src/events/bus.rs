//! In-process broadcast bus for [`Event`]s, tagged by `user_id`.
//! Built on `tokio::sync::broadcast` — one channel for the whole process,
//! every SSE subscriber filters by their own `user_id` so leaks across
//! tenants are impossible.
//! Capacity = 256 — recent messages get dropped if a slow subscriber
//! falls behind. That's fine for a live activity feed (the dashboard can
//! always re-poll for state); raise it if you start losing events the
//! product cares about.

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::events::types::Event;

/// One published message — `(user_id, event)` so subscribers can filter
/// to their tenant cheaply.
pub type Tagged = (String, Event);

/// Cheaply-cloneable handle to the global bus.
/// Clones share the same underlying channel — publish on one, receive on
/// any. The `Arc` wrapping makes the type fit naturally in `AppState`
/// without callers having to think about lifetimes.
#[derive(Clone, Debug)]
pub struct EventBus {
    tx: Arc<broadcast::Sender<Tagged>>,
}

impl EventBus {
    /// Default capacity for the broadcast channel.
    pub const DEFAULT_CAPACITY: usize = 256;

    /// Build a bus with [`Self::DEFAULT_CAPACITY`].
    #[must_use]
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(Self::DEFAULT_CAPACITY);
        Self { tx: Arc::new(tx) }
    }

    /// Build a bus with a custom buffer size. Higher = more memory but
    /// fewer dropped messages under burst load.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        let (tx, _rx) = broadcast::channel(cap);
        Self { tx: Arc::new(tx) }
    }

    /// Publish an event for `user_id`. Returns silently if there are no
    /// subscribers — that's the normal idle case.
    pub fn publish(&self, user_id: impl Into<String>, event: Event) {
        let _ = self.tx.send((user_id.into(), event));
    }

    /// Subscribe to every tagged event. Filter on the receiver side by
    /// matching against the caller's own `user_id`.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Tagged> {
        self.tx.subscribe()
    }

    /// How many subscribers are currently attached. Test/diagnostic use.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::{Event as Ev, TxRef};
    use policy_state::primitives::ChainId;

    fn sample_event(id: &str) -> Event {
        Ev::TxPredicted(TxRef {
            tx_id: id.into(),
            wallet: "0x".into(),
            chain: ChainId::ethereum_mainnet(),
        })
    }

    #[tokio::test]
    async fn publish_then_receive() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        bus.publish("u_alice", sample_event("t1"));
        let (uid, ev) = rx.recv().await.unwrap();
        assert_eq!(uid, "u_alice");
        assert_eq!(ev.kind(), "tx_predicted");
    }

    #[tokio::test]
    async fn multiple_subscribers_all_receive() {
        let bus = EventBus::new();
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.publish("u_x", sample_event("t1"));
        a.recv().await.unwrap();
        b.recv().await.unwrap();
    }

    #[tokio::test]
    async fn no_subscribers_is_not_an_error() {
        // publish without any receiver — must not panic / block.
        let bus = EventBus::new();
        bus.publish("u_x", sample_event("t1"));
    }

    #[tokio::test]
    async fn clones_share_the_same_channel() {
        let bus = EventBus::new();
        let bus2 = bus.clone();
        let mut rx = bus.subscribe();
        bus2.publish("u_x", sample_event("t1"));
        let (uid, _) = rx.recv().await.unwrap();
        assert_eq!(uid, "u_x");
    }
}
