//! Event publication boundary.
//! API handlers should publish through this trait instead of reaching directly
//! into the local process bus. Local development still uses [`EventBus`], while
//! cloud deployments can swap in Redis pub/sub without changing handler logic.

use async_trait::async_trait;

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
