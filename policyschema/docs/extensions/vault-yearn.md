# `vault.yearn` Extension

Yearn V3 vault — ERC-4626 호환 + 추가 보너스 (예: 리워드 분배).

ERC-4626 인터페이스 그대로 사용 + Yearn 특수 함수:
- `setStrategyAuto(...)` (관리자만)
- `expectedShares(uint256 assets)` (preview)

매핑: `VaultDeposit`/`VaultWithdraw`. v0.1 *세미-어댑터 미구현*.
