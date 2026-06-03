//! `migrate` binary — applies Postgres schema migrations, then exits.
//! Runs as the Helm pre-install Job (installed as `policy-server-migrate`).

use policy_server::config::ServerConfig;
use policy_server::storage::StorageBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let config = ServerConfig::from_env();
    policy_server::logging::init_tracing(config.log_format);
    tracing::info!("running policy-server PostgreSQL migrations");
    // `open_with_options(.., true)` connects (with bounded retry) and applies
    // migrations via `POLICY_DB_MIGRATIONS_DIR`.
    let _ = StorageBackend::open_with_options(&config, true).await?;
    tracing::info!("policy-server PostgreSQL migrations complete");
    Ok(())
}
