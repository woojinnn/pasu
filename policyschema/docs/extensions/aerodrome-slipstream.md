# `aerodrome.slipstream` Extension

Aerodrome Slipstream — Aerodrome의 Uniswap V3 fork. **`feeTier`가 아닌 `tickSpacing` 기반**으로 풀을 구분.

## 진입점

| 컨트랙트 | 주소 (Base) |
|---|---|
| SwapRouter | `0xBE6D8f0d05cC4be24d5167a3eF062215bE6D18a5` |
| NonfungiblePositionManager | (v0.1 외) |
| Factory | (CL Pool 도출용) |

## Uniswap V3와의 차이

| 항목 | Uniswap V3 | Aerodrome Slipstream |
|---|---|---|
| 풀 식별 | `feeTier` (uint24) | `tickSpacing` (int24) |
| encoded path 3B | feeTier (uint24 → 3B) | tickSpacing (int24 → 3B) |
| `params` 구조 | `(tokenIn, tokenOut, fee, ...)` | `(tokenIn, tokenOut, tickSpacing, ...)` |

## 주요 함수

| selector | 시그니처 | mode |
|---|---|---|
| `0x...` | `exactInputSingle((address,address,int24,address,uint256,uint256,uint256,uint160))` | ExactIn (single) |
| `0x...` | `exactInput((bytes,address,uint256,uint256,uint256))` | ExactIn (multi) |

## Extension `data` 필드

```jsonc
{
  "namespace": "aerodrome.slipstream",
  "data": {
    "tickSpacing": 100,                  // int24 (음수 가능)
    "tickSpacings": [100, 200],          // 멀티홉
    "encodedPath": "0x...",
    "sqrtPriceLimitX96": "0"
  }
}
```

## fee 추론

`tickSpacing` → fee bps 매핑은 *어댑터 정적 테이블*. 일반적으로:

| tickSpacing | fee bps (추론) |
|---|---|
| 1 | 1 (0.01%) |
| 50 | 5 (0.05%) |
| 100 | 30 (0.3%) |
| 200 | 100 (1%) |

이 매핑이 없으면 `max_fee_bps = None` + confidence ceiling = `medium`.

## 참고사항

- 풀 주소 도출은 V3와 동일 패턴(`PoolAddress.computeAddress` with tickSpacing).
- Aerodrome V1은 별도 namespace.
