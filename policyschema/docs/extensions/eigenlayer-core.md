# `eigenlayer.core` Extension

EigenLayer 재스테이킹 프레임워크의 코어 컨트랙트들.

| 진입점 | 주소 (mainnet) |
|---|---|
| StrategyManager | `0x858646372CC42E1A627fcE94aa7A7033e7CF075A` |
| DelegationManager | `0x39053D51B77DC0d36036Fc1fCc8Cb819df8Ef37A` |

핵심: `depositIntoStrategy(IStrategy, IERC20, uint256)`, `delegateTo(address operator, ...)`, `queueWithdrawals(...)`, `completeQueuedWithdrawals(...)`.

매핑: ActionType `Restake`/`DelegateOperator`/`Undelegate`/`QueueWithdrawal`/`ClaimWithdrawal`. v0.1 *세미-어댑터 미구현*.
