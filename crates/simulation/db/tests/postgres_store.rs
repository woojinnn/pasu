#![cfg(feature = "postgres")]

use simulation_db::stores::{PostgresGlobalDb, PostgresWalletStore};
use simulation_state::{WalletId, WalletState, WalletStore};

#[tokio::test]
async fn postgres_wallet_store_round_trips_state() {
    let url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => return,
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let global = PostgresGlobalDb::new(pool.clone());
    global.migrate().await.unwrap();

    let user_id = global
        .upsert_user("alice@example.com", "google")
        .await
        .unwrap();
    let store = PostgresWalletStore::new(pool, user_id);
    let id: WalletId = serde_json::from_value(serde_json::json!({
        "address": "0x362E7e9e630481631D7C804dfe50e24b53250925",
        "chains": ["eip155:1"]
    }))
    .unwrap();

    let state = WalletState::new(id.clone());
    store.save(&state).await.unwrap();
    let loaded = store.load(&id).await.unwrap();

    assert_eq!(loaded.wallet_id, id);
}
