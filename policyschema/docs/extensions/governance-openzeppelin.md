# `governance.openzeppelin` Extension

OpenZeppelin Governor — `GovernorVotesQuorumFraction` + `GovernorTimelockControl` 등 모듈 합성.

Governor Bravo와 비슷하지만 인터페이스 약간 차이:
- `propose(targets[], values[], calldatas[], description)` (signatures 없음)
- `castVoteWithReasonAndParams(...)` 추가

매핑은 `governance.governorBravo`와 동일 ActionType. v0.1 *세미-어댑터 미구현*.
