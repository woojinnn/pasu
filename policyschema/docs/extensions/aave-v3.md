# `aave.v3` Extension

Aave Protocol V3 — 단일 `Pool` 컨트랙트가 supply / withdraw / borrow / repay 4 핵심 함수를 처리.

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| Pool (Proxy) | `0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2` |
| PoolAddressesProvider | `0x2f39d218133AFaB8F2B819B1066c7E434Ad94E9e` |

(다른 체인의 Pool 주소는 `PoolAddressesProvider.getPool()`로 도출)

## v0.1이 다루는 함수 (이자 핵심 4종)

| selector | 시그니처 | ActionType |
|---|---|---|
| `0x617ba037` | `supply(address,uint256,address,uint16)` | Supply |
| `0x69328dec` | `withdraw(address,uint256,address)` | Withdraw |
| `0xa415bcad` | `borrow(address,uint256,uint256,uint16,address)` | Borrow |
| `0x573ade81` | `repay(address,uint256,uint256,address)` | Repay |

## 인자 매핑

### `supply(asset, amount, onBehalfOf, referralCode)`
- `asset` → `LendingFields.asset`
- `amount` → `LendingFields.amount.raw`, kind = `Exact`
- `onBehalfOf` → `LendingFields.on_behalf_of`
- `referralCode` → Extension data

### `withdraw(asset, amount, to)`
- `asset` → `LendingFields.asset`
- `amount` → `LendingFields.amount.raw` — `type(uint256).max`이면 kind = `Unlimited` (전액 인출)
- `to` → `LendingFields.recipients.recipient`

### `borrow(asset, amount, interestRateMode, referralCode, onBehalfOf)`
- `interestRateMode` (uint256: 1=Stable, 2=Variable) → `LendingFields.interest_rate_mode`
- 나머지 동일

### `repay(asset, amount, rateMode, onBehalfOf)`
- `amount` → `type(uint256).max`이면 kind = `Unlimited` (전액 상환)
- `rateMode` → `LendingFields.interest_rate_mode`

## Extension `data` 필드

```jsonc
{
  "namespace": "aave.v3",
  "data": {
    "referralCode": 0                     // supply / borrow에 존재
  }
}
```

## 참고사항

- Aave V3에는 `setUserUseReserveAsCollateral`, `flashLoan`, `liquidationCall` 등 부가 함수가 있지만 **v0.1 범위 외**.
- aTokens (예: aUSDC) 직접 transfer는 ERC20 transfer로 분류 — 별도 ActionType 안 부여.
