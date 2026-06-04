//! Tracing initialization shared by the server and worker binaries.

use tracing_subscriber::EnvFilter;

use crate::config::LogFormat;

/// Initialize the global tracing subscriber in the selected format.
/// `LOG_FORMAT=json` emits one JSON object per line for GKE Cloud Logging;
/// anything else stays human-readable for local dev.
pub fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,policy_server=debug"));
    match format {
        LogFormat::Json => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init(),
        LogFormat::Human => tracing_subscriber::fmt().with_env_filter(filter).init(),
    }
}
