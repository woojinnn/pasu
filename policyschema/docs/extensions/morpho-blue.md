# `morpho.blue` Extension

Morpho Blue — 미니멀 렌딩 프리미티브. `marketParams`로 시장 식별 (Aave처럼 단일 Pool이 아니라, 시장이 동적으로 생성).

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| Morpho Blue | `0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFCb` |

## marketParams 5튜플

```solidity
struct MarketParams {
    address loanToken;       // 빌릴 자산
    address collateralToken; // 담보 자산
    address oracle;
    address irm;             // interest rate model
    uint256 lltv;            // liquidation LTV (basis points × 1e16)
}
```

`marketId = keccak256(abi.encode(marketParams))`로 도출.

## v0.1이 다루는 함수 (이자 핵심 4종)

| 시그니처 | ActionType |
|---|---|
| `supply(MarketParams, uint256 assets, uint256 shares, address onBehalf, bytes data)` | Supply |
| `withdraw(MarketParams, uint256 assets, uint256 shares, address onBehalf, address receiver)` | Withdraw |
| `borrow(MarketParams, uint256 assets, uint256 shares, address onBehalf, address receiver)` | Borrow |
| `repay(MarketParams, uint256 assets, uint256 shares, address onBehalf, bytes data)` | Repay |

## 인자 매핑

### `supply(marketParams, assets, shares, onBehalf, data)`
- `marketParams.loanToken` → `LendingFields.asset`
- `assets` 또는 `shares` (둘 중 하나만 nonzero — 둘 다 nonzero면 revert) → `LendingFields.amount.raw`
  - `assets` 사용 시 kind = `Exact`
  - `shares` 사용 시 kind = `Exact` 단, raw 값은 shares 단위 (별도 의미)
- `onBehalf` → `LendingFields.on_behalf_of`
- `data` → Extension data (콜백용 임의 바이트)

### `withdraw(marketParams, assets, shares, onBehalf, receiver)`
- `receiver` → `LendingFields.recipients.recipient`

### `borrow` / `repay` — 위와 동일 패턴

## Extension `data` 필드

```jsonc
{
  "namespace": "morpho.blue",
  "data": {
    "marketParams": {
      "loanToken": "0x...",
      "collateralToken": "0x...",
      "oracle": "0x...",
      "irm": "0x...",
      "lltv": "860000000000000000"           // uint256 decimal string (LTV × 1e18)
    },
    "shares": "0",                            // 위 Borrow/Withdraw용 — assets 또는 이 값이 nonzero
    "data": "0x"                              // supply/repay의 callback bytes
  }
}
```

## 참고사항

- **이자율 모드 없음**: Morpho Blue는 variable rate 단일 — `LendingFields.interest_rate_mode = None`.
- **Repay-max 패턴**: `shares = type(uint256).max` 또는 `assets = type(uint256).max`로 전액 상환 가능 → `amount.kind = Unlimited`.
- **`marketId`** 자체는 schema에 surface하지 않음 (도출 가능). 정책에서 필요하면 `marketParams`로 비교.
