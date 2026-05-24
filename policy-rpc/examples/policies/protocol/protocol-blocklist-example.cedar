@id("user/protocol-blocklist-example")
@severity("deny")
@reason("PoC 예시 — 특정 protocol contract address (hardcoded) 차단. 실제 사용 시 사용자 own list 작성.")
forbid (
  principal,
  action,
  resource == Protocol::"0x000000000000000000000000000000000000dead"
);
