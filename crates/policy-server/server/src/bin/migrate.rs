use policy_db::stores::postgres::connect_pool;
use policy_db::GlobalDb;
use policy_server::config::ServerConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,policy_server=debug")),
        )
        .init();

    let config = ServerConfig::from_env();
    let database_url = config.database_url.as_deref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "DATABASE_URL is required")
    })?;

    tracing::info!("running policy-server PostgreSQL migrations");
    let pool = connect_pool(database_url).await?;
    let global_db = GlobalDb::new(pool);
    global_db.migrate().await?;
    tracing::info!("policy-server PostgreSQL migrations complete");
    Ok(())
}
