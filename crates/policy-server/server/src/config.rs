//! Runtime configuration for the policy server.
//! Cloud deployments should inject these values through environment
//! variables. Tests use [`ServerConfig::for_tests`] so router behavior can be
//! exercised without mutating process-wide env for every case.

use std::env;

/// Log output format selected by `LOG_FORMAT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable lines (local dev default).
    Human,
    /// One JSON object per line (GKE Cloud Logging).
    Json,
}

impl LogFormat {
    fn from_env_value(v: &str) -> Self {
        if v.eq_ignore_ascii_case("json") {
            Self::Json
        } else {
            Self::Human
        }
    }
}

/// Typed runtime configuration shared by the API server and worker processes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    /// Socket address the API process binds to.
    pub bind_addr: String,
    /// Public dashboard origin used by OAuth redirects and default CORS.
    pub dashboard_url: String,
    /// Public API URL advertised to browser clients.
    pub public_api_url: String,
    /// Exact origins that may call authenticated HTTP APIs from a browser.
    pub cors_allowed_origins: Vec<String>,
    /// Whether to emit the private-network CORS approval header.
    pub allow_private_network: bool,
    /// Durable `PostgreSQL` database URL. Required by process startup.
    pub database_url: Option<String>,
    /// Redis URL for coordination/fanout. `None` keeps in-process dev mode.
    pub redis_url: Option<String>,
    /// Whether API/worker startup should apply database migrations.
    pub run_migrations_on_startup: bool,
    /// Whether a missing or invalid sync config makes readiness fail.
    pub require_sync_config: bool,
    /// TTL for distributed sync locks, in seconds.
    pub sync_lock_ttl_secs: u64,
    /// Redis pub/sub channel used for cross-replica event fanout.
    pub redis_events_channel: String,
    /// Max Postgres pool connections per process.
    pub db_max_connections: u32,
    /// Seconds to wait for a pool connection before erroring.
    pub db_acquire_timeout_secs: u64,
    /// Max startup DB-connect retry attempts before giving up.
    pub db_connect_max_retries: u32,
    /// Base backoff (seconds) between startup DB-connect retries.
    pub db_connect_backoff_secs: u64,
    /// Tracing output format.
    pub log_format: LogFormat,
}

impl ServerConfig {
    /// Load configuration from environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        let sync_worker_tick_secs = env_u64("SYNC_WORKER_TICK_SECS", 30);
        Self {
            bind_addr: env::var("POLICY_SERVER_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8788".to_owned()),
            dashboard_url: env::var("DASHBOARD_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5173".to_owned()),
            public_api_url: env::var("PUBLIC_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8788".to_owned()),
            cors_allowed_origins: env::var("CORS_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| ["http://127.0.0.1:5173", "http://localhost:5173"].join(","))
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            allow_private_network: env::var("CORS_ALLOW_PRIVATE_NETWORK")
                .map_or(true, |v| v == "1" || v.eq_ignore_ascii_case("true")),
            database_url: env::var("DATABASE_URL").ok(),
            redis_url: env::var("REDIS_URL").ok(),
            run_migrations_on_startup: env_bool("RUN_MIGRATIONS_ON_STARTUP", true),
            require_sync_config: env_bool("REQUIRE_SYNC_CONFIG", false),
            sync_lock_ttl_secs: env::var("SYNC_LOCK_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| (sync_worker_tick_secs * 4).max(120)),
            redis_events_channel: env::var("REDIS_EVENTS_CHANNEL")
                .unwrap_or_else(|_| "policy-server:events".to_owned()),
            db_max_connections: env_u32("DB_MAX_CONNECTIONS", 10),
            db_acquire_timeout_secs: env_u64("DB_ACQUIRE_TIMEOUT_SECS", 10),
            db_connect_max_retries: env_u32("DB_CONNECT_MAX_RETRIES", 12),
            db_connect_backoff_secs: env_u64("DB_CONNECT_BACKOFF_SECS", 5),
            log_format: env::var("LOG_FORMAT")
                .map_or(LogFormat::Human, |v| LogFormat::from_env_value(&v)),
        }
    }

    /// Deterministic defaults for integration tests.
    #[must_use]
    pub fn for_tests() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_owned(),
            dashboard_url: "http://127.0.0.1:5173".to_owned(),
            public_api_url: "http://127.0.0.1:8788".to_owned(),
            cors_allowed_origins: vec!["http://127.0.0.1:5173".to_owned()],
            allow_private_network: true,
            database_url: env::var("TEST_DATABASE_URL").ok().or_else(|| {
                Some("postgres://scopeball:scopeball@127.0.0.1:5432/scopeball_test".to_owned())
            }),
            redis_url: None,
            run_migrations_on_startup: true,
            require_sync_config: false,
            sync_lock_ttl_secs: 120,
            redis_events_channel: "policy-server:test-events".to_owned(),
            db_max_connections: 5,
            db_acquire_timeout_secs: 10,
            db_connect_max_retries: 1,
            db_connect_backoff_secs: 1,
            log_format: LogFormat::Human,
        }
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name).map_or(default, |v| {
        v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
    })
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::{env_u32, LogFormat, ServerConfig};

    #[test]
    fn env_u32_parses_and_defaults() {
        std::env::remove_var("POLICY_TEST_DB_MAX_CONN");
        assert_eq!(env_u32("POLICY_TEST_DB_MAX_CONN", 10), 10);
        std::env::set_var("POLICY_TEST_DB_MAX_CONN", "25");
        assert_eq!(env_u32("POLICY_TEST_DB_MAX_CONN", 10), 25);
        std::env::remove_var("POLICY_TEST_DB_MAX_CONN");
    }

    #[test]
    fn for_tests_has_pool_defaults() {
        let c = ServerConfig::for_tests();
        assert_eq!(c.db_max_connections, 5);
        assert_eq!(c.db_connect_max_retries, 1);
        assert_eq!(c.log_format, LogFormat::Human);
    }
}
