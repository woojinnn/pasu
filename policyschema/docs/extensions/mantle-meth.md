# `mantle.meth` Extension

Mantle mETH — Mantle의 ETH 액화 스테이킹.

| 진입점 | 주소 (mainnet) |
|---|---|
| Staking | `0xe3cBd06D7dadB3F4e6557bAb7EdD924CD1489E8f` |
| mETH | `0xd5F7838F5C461fefF7FE49ea5ebaF7728bB0ADfa` |

핵심: `stake(uint256 minMETHAmount)` payable, `unstakeRequest(uint128 mETHAmount, uint128 minETHAmount)`, `claimUnstakeRequest(uint256 unstakeRequestId)`.

v0.1 *세미-어댑터 미구현*.
