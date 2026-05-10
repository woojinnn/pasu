# `safe` Extension

Gnosis Safe (Account Abstraction) — multisig 지갑 + AA 7702 호환.

| 진입점 | 주소 |
|---|---|
| Safe (proxy factory) | `0xa6B71E26C5e0845f74c812102Ca7114b6a896AB2` |
| SafeProxy | per-Safe deploy |

핵심:
- `execTransaction(...)` — Safe 트랜잭션 실행 (오너 서명 모음)
- typed-data `SafeTx` 서명 → `SignSafeTx` ActionType

```jsonc
{
  "namespace": "safe",
  "data": {
    "safe": "0x...",
    "threshold": 2,
    "ownersInvolved": ["0x...", "0x..."],
    "nonce": "5"
  }
}
```

v0.1 *세미-어댑터 미구현* (sign decoder는 SafeTx 일부 처리).
