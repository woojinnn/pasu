//! Event publication boundary.
//! API handlers should publish through this trait instead of reaching directly
//! into the local process bus. Local development still uses [`EventBus`], while
//! cloud deployments can swap in Redis pub/sub without changing handler logic.

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::events::{Event, EventBus};

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, user_id: String, event: Event);
}

#[derive(Clone)]
pub struct LocalEventPublisher {
    bus: EventBus,
}

impl LocalEventPublisher {
    #[must_use]
    pub const fn new(bus: EventBus) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl EventPublisher for LocalEventPublisher {
    async fn publish(&self, user_id: String, event: Event) {
        self.bus.publish(user_id, event);
    }
}

/// Wire payload published through Redis pub/sub.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedisEventEnvelope {
    pub user_id: String,
    pub event: Event,
}

/// Redis-backed publisher for cross-replica event fanout.
#[derive(Clone)]
pub struct RedisEventPublisher {
    manager: redis::aio::ConnectionManager,
    channel: String,
}

impl RedisEventPublisher {
    /// Connect to Redis and publish events to `channel`.
    ///
    /// # Errors
    ///
    /// Returns the underlying Redis error if the client or initial connection
    /// cannot be created.
    pub async fn connect(url: &str, channel: impl Into<String>) -> redis::RedisResult<Self> {
        let client = redis::Client::open(url)?;
        let manager = client.get_connection_manager().await?;
        Ok(Self {
            manager,
            channel: channel.into(),
        })
    }
}

#[async_trait]
impl EventPublisher for RedisEventPublisher {
    async fn publish(&self, user_id: String, event: Event) {
        let envelope = RedisEventEnvelope { user_id, event };
        let Ok(payload) = serde_json::to_string(&envelope) else {
            tracing::warn!("failed to serialize Redis event envelope");
            return;
        };
        let mut conn = self.manager.clone();
        if let Err(e) = redis::cmd("PUBLISH")
            .arg(&self.channel)
            .arg(payload)
            .query_async::<i64>(&mut conn)
            .await
        {
            tracing::warn!(error = %e, channel = %self.channel, "failed to publish Redis event");
        }
    }
}

/// Subscribe to Redis events and forward them into the local process bus used
/// by SSE connections.
///
/// # Errors
///
/// Returns the underlying Redis error if the client cannot connect or subscribe.
pub async fn spawn_redis_event_forwarder(
    redis_url: &str,
    channel: impl Into<String>,
    bus: EventBus,
) -> redis::RedisResult<tokio::task::JoinHandle<()>> {
    let channel = channel.into();
    let client = redis::Client::open(redis_url)?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(&channel).await?;
    let mut stream = pubsub.into_on_message();

    Ok(tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            let payload: redis::RedisResult<String> = msg.get_payload();
            if let Some(envelope) = payload
                .ok()
                .and_then(|s| serde_json::from_str::<RedisEventEnvelope>(&s).ok())
            {
                bus.publish(envelope.user_id, envelope.event);
            } else {
                tracing::warn!(channel = %channel, "failed to decode Redis event");
            }
        }
        tracing::warn!(channel = %channel, "Redis event forwarder stopped");
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::{Event as Ev, TxRef};
    use policy_state::primitives::ChainId;

    #[test]
    fn redis_event_envelope_round_trips_json() {
        let envelope = RedisEventEnvelope {
            user_id: "u_alice".to_owned(),
            event: Ev::TxPredicted(TxRef {
                tx_id: "t1".to_owned(),
                wallet: "0xabc".to_owned(),
                chain: ChainId::ethereum_mainnet(),
            }),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let back: RedisEventEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(back, envelope);
        assert_eq!(back.event.kind(), "tx_predicted");
    }
}
