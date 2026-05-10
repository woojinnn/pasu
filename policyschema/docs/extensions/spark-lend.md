# `spark.lend` Extension

Spark Lend — Aave V3 fork (MakerDAO 생태계).

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| Pool (Proxy) | `0xC13e21B648A5Ee794902342038FF3aDAB66BE987` |

## 핵심 함수

Aave V3와 *동일 selector* 사용 (fork이므로). 매핑은 `aave.v3` 동일.

| 시그니처 | ActionType |
|---|---|
| `supply(asset, amount, onBehalfOf, referralCode)` | Supply |
| `withdraw(asset, amount, to)` | WithdrawCollateral |
| `borrow(asset, amount, interestRateMode, referralCode, onBehalfOf)` | Borrow |
| `repay(asset, amount, rateMode, onBehalfOf)` | Repay |

## Extension `data` 필드

```jsonc
{
  "namespace": "spark.lend",
  "data": {
    "referralCode": 0,
    "isSdai": false              // sDAI/USDS 통합 작업이면 true
  }
}
```

## 참고

- **selector 충돌**: Aave V3와 동일. dispatch는 `target_address`로 분기 (Aave Pool ↔ Spark Pool 다름).
- v0.1 *세미-어댑터 미구현* — 데이터 모델만.
