//! Kubernetes readiness checks.
//!
//! `/health` remains a cheap liveness probe. `/readyz` verifies that this
//! process can serve real traffic: durable storage is reachable, required
//! secrets are present, sync config policy is satisfied, and Redis responds
//! when configured.

use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use policy_sync::SyncConfig;

use crate::app::AppState;
use crate::config::ServerConfig;

const DEFAULT_SYNC_CONFIG: &str = "./pasu-sync.toml";

/// JSON body returned by `/readyz`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReadinessReport {
    pub status: &'static str,
    pub checks: BTreeMap<&'static str, String>,
}

/// `GET /readyz` — readiness probe for Kubernetes traffic routing.
pub async fn readyz_handler(State(state): State<AppState>) -> Response {
    let config = ServerConfig::from_env();
    let mut checks = BTreeMap::new();

    checks.insert(
        "required_env",
        required_env_status(&["DATABASE_URL", "JWT_SECRET"]),
    );
    checks.insert("postgres", postgres_status(&state).await);
    checks.insert("sync_config", sync_config_status(&config));
    checks.insert("redis", redis_status(config.redis_url.as_deref()).await);

    let ready = checks
        .values()
        .all(|status| status == "ok" || status == "skipped");
    let report = ReadinessReport {
        status: if ready { "ready" } else { "not_ready" },
        checks,
    };
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(report)).into_response()
}

fn required_env_status(names: &[&str]) -> String {
    let missing: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| std::env::var(name).map_or(true, |v| v.trim().is_empty()))
        .collect();
    if missing.is_empty() {
        "ok".to_owned()
    } else {
        format!("missing:{}", missing.join(","))
    }
}

async fn postgres_status(state: &AppState) -> String {
    state
        .global_db
        .ping()
        .await
        .map_or_else(|e| format!("error:{e}"), |()| "ok".to_owned())
}

fn sync_config_status(config: &ServerConfig) -> String {
    let path = std::env::var("PASU_SYNC_CONFIG")
        .map_or_else(|_| PathBuf::from(DEFAULT_SYNC_CONFIG), PathBuf::from);
    match SyncConfig::load_file(&path) {
        Ok(_) => "ok".to_owned(),
        Err(e) if config.require_sync_config => format!("error:{e}"),
        Err(_) => "skipped".to_owned(),
    }
}

async fn redis_status(redis_url: Option<&str>) -> String {
    let Some(url) = redis_url.filter(|url| !url.trim().is_empty()) else {
        return "skipped".to_owned();
    };
    let client = match redis::Client::open(url) {
        Ok(client) => client,
        Err(e) => return format!("error:{e}"),
    };
    let mut conn = match client.get_connection_manager().await {
        Ok(conn) => conn,
        Err(e) => return format!("error:{e}"),
    };
    redis::cmd("PING")
        .query_async::<String>(&mut conn)
        .await
        .map_or_else(|e| format!("error:{e}"), |_| "ok".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_env_reports_missing_names() {
        std::env::remove_var("POLICY_SERVER_TEST_REQUIRED_ENV");
        let status = required_env_status(&["POLICY_SERVER_TEST_REQUIRED_ENV"]);
        assert_eq!(status, "missing:POLICY_SERVER_TEST_REQUIRED_ENV");
    }
}
