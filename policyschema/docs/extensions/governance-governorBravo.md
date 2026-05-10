# `governance.governorBravo` Extension

Compound Governor Bravo 패턴 — Compound·Uniswap UNI·기타 클론.

핵심 함수:
- `propose(targets[], values[], signatures[], calldatas[], description)` → `GovernancePropose`
- `castVote(proposalId, support)` / `castVoteWithReason(proposalId, support, reason)` → `GovernanceVote`
- `execute(proposalId)` / `queue(proposalId)` → `GovernanceExecute`

```jsonc
{
  "namespace": "governance.governorBravo",
  "data": {
    "governor": "0x...",
    "proposalId": "12",
    "support": 1,                // 0=against, 1=for, 2=abstain
    "votingPower": "1000000000000000000000"
  }
}
```

v0.1 *세미-어댑터 미구현*.
