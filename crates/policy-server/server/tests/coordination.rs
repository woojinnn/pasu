use std::time::Duration;

use simulation_server::coordination::{Coordinator, NoopCoordinator};

#[tokio::test]
async fn noop_coordinator_allows_lock_and_idempotency() {
    let c = NoopCoordinator;
    let token = c
        .try_lock("lock:wallet:u:0x1", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(token.key, "lock:wallet:u:0x1");
    assert!(c
        .mark_idempotent("report:abc", Duration::from_secs(60))
        .await
        .unwrap());
    c.release_lock(token).await.unwrap();
}
