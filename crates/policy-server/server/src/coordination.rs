//! Cross-replica coordination primitives.
//! Redis is used only for short-lived coordination: locks and idempotency
//! keys. It is not the source of truth for users, wallets, reports, or
//! canonical wallet state.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

/// Errors from the coordination backend.
#[derive(Debug, thiserror::Error)]
pub enum CoordinationError {
    /// Redis command or connection failure.
    #[error("redis: {0}")]
    Redis(String),
}

/// Shared coordination contract used by API replicas and worker processes.
#[async_trait]
pub trait Coordinator: Send + Sync {
    /// Try to acquire a TTL-bound lock. `Ok(None)` means another replica owns
    /// the key.
    async fn try_lock(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<Option<LockToken>, CoordinationError>;

    /// Release a lock only when the token still matches.
    async fn release_lock(&self, token: LockToken) -> Result<(), CoordinationError>;

    /// Mark a key as seen for `ttl`. Returns `true` only for the first caller.
    async fn mark_idempotent(&self, key: &str, ttl: Duration) -> Result<bool, CoordinationError>;
}

/// Opaque value proving ownership of a lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockToken {
    /// Redis lock key.
    pub key: String,
    /// Random value set by the lock owner.
    pub value: String,
}

/// Development coordinator. It never contends, so local single-process tests
/// can exercise worker code without Redis.
#[derive(Clone, Default)]
pub struct NoopCoordinator;

#[async_trait]
impl Coordinator for NoopCoordinator {
    async fn try_lock(
        &self,
        key: &str,
        _ttl: Duration,
    ) -> Result<Option<LockToken>, CoordinationError> {
        Ok(Some(LockToken {
            key: key.to_owned(),
            value: "noop".to_owned(),
        }))
    }

    async fn release_lock(&self, _token: LockToken) -> Result<(), CoordinationError> {
        Ok(())
    }

    async fn mark_idempotent(&self, _key: &str, _ttl: Duration) -> Result<bool, CoordinationError> {
        Ok(true)
    }
}

/// Redis-backed coordinator for cloud replicas.
#[derive(Clone)]
pub struct RedisCoordinator {
    manager: redis::aio::ConnectionManager,
}

impl RedisCoordinator {
    /// Connect to Redis using a standard `redis://` URL.
    pub async fn connect(url: &str) -> Result<Self, CoordinationError> {
        let client =
            redis::Client::open(url).map_err(|e| CoordinationError::Redis(e.to_string()))?;
        let manager = client
            .get_connection_manager()
            .await
            .map_err(|e| CoordinationError::Redis(e.to_string()))?;
        Ok(Self { manager })
    }
}

#[async_trait]
impl Coordinator for RedisCoordinator {
    async fn try_lock(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<Option<LockToken>, CoordinationError> {
        let mut conn = self.manager.clone();
        let value = uuid::Uuid::new_v4().to_string();
        let millis = ttl_millis(ttl);
        let ok: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(&value)
            .arg("NX")
            .arg("PX")
            .arg(millis)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Redis(e.to_string()))?;
        Ok(ok.map(|_| LockToken {
            key: key.to_owned(),
            value,
        }))
    }

    async fn release_lock(&self, token: LockToken) -> Result<(), CoordinationError> {
        let mut conn = self.manager.clone();
        let script = redis::Script::new(
            r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
              return redis.call("DEL", KEYS[1])
            else
              return 0
            end
            "#,
        );
        script
            .key(token.key)
            .arg(token.value)
            .invoke_async::<i64>(&mut conn)
            .await
            .map_err(|e| CoordinationError::Redis(e.to_string()))?;
        Ok(())
    }

    async fn mark_idempotent(&self, key: &str, ttl: Duration) -> Result<bool, CoordinationError> {
        let mut conn = self.manager.clone();
        let millis = ttl_millis(ttl);
        let value: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("PX")
            .arg(millis)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Redis(e.to_string()))?;
        Ok(value.is_some())
    }
}

/// Shared coordinator trait object.
pub type DynCoordinator = Arc<dyn Coordinator>;

fn ttl_millis(ttl: Duration) -> u64 {
    u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX)
}
