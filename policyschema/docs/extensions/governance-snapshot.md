# `governance.snapshot` Extension

Snapshot — 오프체인 voting (EIP-712 서명 기반, 가스 무료).

`SignSnapshotVote` typed-data 구조 — 도메인 = `snapshot`. `castVote`는 트랜잭션이 아니라 *서명*이므로 `Sign` 카테고리에 가까움. 정책상 `Governance` 카테고리로 분류해 정책 표현 단순화.

```jsonc
{
  "namespace": "governance.snapshot",
  "data": {
    "spaceId": "uniswap.eth",
    "proposalCid": "Qm...",
    "choice": 1
  }
}
```

v0.1 *세미-어댑터 미구현*.
