use simulation_server::config::ServerConfig;
use simulation_server::storage::StorageBackend;
use simulation_state::primitives::{Address, ChainId};
use simulation_state::{WalletId, WalletState};
use std::str::FromStr;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL PostgreSQL integration database"]
async fn storage_backend_lists_users_and_wallet_stores_for_worker() {
    let config = ServerConfig::for_tests();
    let storage = StorageBackend::open(&config).await.unwrap();
    let user_id = storage
        .global_db()
        .upsert_user("worker@example.com", "google")
        .await
        .unwrap();
    let store = storage.wallet_store_for_user(&user_id).unwrap();
    let wallet_id = WalletId::new(
        Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        [ChainId::ethereum_mainnet()],
    );
    store
        .save(&WalletState::new(wallet_id.clone()))
        .await
        .unwrap();

    assert_eq!(storage.list_user_ids().await.unwrap(), vec![user_id]);
    assert_eq!(store.list_wallets().await.unwrap(), vec![wallet_id]);
}
