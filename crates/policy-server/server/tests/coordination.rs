use std::time::Duration;

use policy_server::config::ServerConfig;
use policy_server::coordination::{build_coordinator, Coordinator, NoopCoordinator};

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

#[tokio::test]
async fn coordinator_builder_uses_noop_without_redis_url() {
    let mut config = ServerConfig::for_tests();
    config.redis_url = None;

    let c = build_coordinator(&config).await.unwrap();
    let token = c
        .try_lock("sync:user:u_alice", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(token.value, "noop");
}

#[tokio::test]
async fn coordinator_builder_rejects_invalid_redis_url() {
    let mut config = ServerConfig::for_tests();
    config.redis_url = Some("not a redis url".to_owned());

    let Err(err) = build_coordinator(&config).await else {
        panic!("invalid Redis URL should fail");
    };
    assert!(err.to_string().contains("redis:"));
}
