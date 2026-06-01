//! Runtime configuration for the policy server.
//! Cloud deployments should inject these values through environment
//! variables. Tests use [`ServerConfig::for_tests`] so router behavior can be
//! exercised without mutating process-wide env for every case.

use std::env;

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
}

impl ServerConfig {
    /// Load configuration from environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            bind_addr: env::var("POLICY_SERVER_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8788".to_owned()),
            dashboard_url: env::var("DASHBOARD_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5174".to_owned()),
            public_api_url: env::var("PUBLIC_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8788".to_owned()),
            cors_allowed_origins: env::var("CORS_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| {
                    [
                        "http://127.0.0.1:5174",
                        "http://localhost:5174",
                        "http://127.0.0.1:5175",
                        "http://localhost:5175",
                    ]
                    .join(",")
                })
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            allow_private_network: env::var("CORS_ALLOW_PRIVATE_NETWORK")
                .map_or(true, |v| v == "1" || v.eq_ignore_ascii_case("true")),
            database_url: env::var("DATABASE_URL").ok(),
            redis_url: env::var("REDIS_URL").ok(),
        }
    }

    /// Deterministic defaults for integration tests.
    #[must_use]
    pub fn for_tests() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_owned(),
            dashboard_url: "http://127.0.0.1:5174".to_owned(),
            public_api_url: "http://127.0.0.1:8788".to_owned(),
            cors_allowed_origins: vec!["http://127.0.0.1:5174".to_owned()],
            allow_private_network: true,
            database_url: env::var("TEST_DATABASE_URL").ok().or_else(|| {
                Some("postgres://scopeball:scopeball@127.0.0.1:5432/scopeball_test".to_owned())
            }),
            redis_url: None,
        }
    }
}
