# `eigenlayer.eigenpod` Extension

EigenLayer EigenPod — native ETH 재스테이킹 (validator BLS 키 제공).

| 진입점 | 주소 |
|---|---|
| EigenPodManager | `0x91E677b07F7AF907ec9a428aafA9fc14a0d3A338` |
| EigenPod (per-staker) | factory deploy |

핵심: `createPod()`, `verifyWithdrawalCredentials(...)`, `verifyAndProcessWithdrawals(...)`. BLS 증명 인자가 매우 길음 — confidence `medium` ceiling.

v0.1 *세미-어댑터 미구현*.
