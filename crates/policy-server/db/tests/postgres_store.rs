use policy_db::derive_user_id;
use policy_db::stores::{PostgresGlobalDb, PostgresWalletStore};
use policy_state::{WalletId, WalletState, WalletStore};
use sqlx_postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

#[tokio::test]
async fn postgres_wallet_store_round_trips_state() {
    let url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => return,
    };
    let pool = PgPool::connect(&url).await.unwrap();
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

/// Concurrent first-time logins of the SAME email must all succeed and return
/// the same id. `users` has two unique constraints (`users_pkey` on user_id and
/// `users_email_key` on email); a `upsert_user` that arbitrates `ON CONFLICT`
/// on only one of them races to insert the other and fails with a duplicate-key
/// 500 instead of upserting idempotently.
///
/// The race only fires on the *first* insert for an email (an existing row makes
/// every caller skip the conflicting INSERT), so each burst uses a fresh email
/// and we loop many bursts to make the narrow window reproduce reliably.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upsert_user_is_idempotent_under_concurrent_first_login() {
    let url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => return,
    };
    // Enough connections that the concurrent INSERTs genuinely overlap at the
    // server instead of serializing through a small pool.
    let pool = PgPoolOptions::new()
        .max_connections(64)
        .connect(&url)
        .await
        .unwrap();
    let global = PostgresGlobalDb::new(pool);
    global.migrate().await.unwrap();

    const BURSTS: usize = 30;
    const CONCURRENCY: usize = 48;
    for _ in 0..BURSTS {
        let email = format!("race-{}@example.com", Uuid::new_v4());
        let expected = derive_user_id(&email);

        let mut handles = Vec::with_capacity(CONCURRENCY);
        for _ in 0..CONCURRENCY {
            let global = global.clone();
            let email = email.clone();
            handles.push(tokio::spawn(async move {
                global.upsert_user(&email, "google").await
            }));
        }

        for handle in handles {
            let id = handle
                .await
                .expect("upsert_user task panicked")
                .expect("upsert_user must succeed idempotently under concurrency");
            assert_eq!(id, expected, "all concurrent logins must yield the same id");
        }
    }
}
