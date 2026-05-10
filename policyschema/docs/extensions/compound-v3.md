# `compound.v3` Extension

Compound V3 (Comet) — 단일 borrowable asset (USDC 등) + 다중 collateral.

## 진입점

각 마켓이 별도 Comet 컨트랙트 (mainnet USDC 마켓: `0xc3d688B66703497DAA19211EEdff47f25384cdc3`).

## 핵심 함수

```solidity
function supply(address asset, uint256 amount) external;        // collateral 또는 base
function withdraw(address asset, uint256 amount) external;      // base 인출 = borrow
function withdrawTo(address to, address asset, uint256 amount) external;
function transferAsset(address dst, address asset, uint256 amount) external;
```

## Extension `data` 필드

```jsonc
{
  "namespace": "compound.v3",
  "data": {
    "comet": "0x...",            // 마켓 컨트랙트
    "isBase": true                // base 자산이면 borrow/repay; 아니면 collateral
  }
}
```

## 참고

- **borrow vs withdraw**: Compound V3는 *base 자산 withdraw*가 곧 borrow. Aave와 다른 의미론.
- v0.1 *세미-어댑터 미구현*.
