# `uniswap.v3` Extension

Uniswap V3 — concentrated liquidity AMM. fee-tier별 풀, encoded path 멀티홉.

## 진입점

| 컨트랙트 | 주소 (mainnet) | 역할 |
|---|---|---|
| SwapRouter | `0xE592427A0AEce92De3Edee1F18E0157C05861564` | V3 첫 라우터 |
| SwapRouter02 | `0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45` | UR 도입 직전 라우터 |
| UniversalRouter | (별도 namespace `uniswap.universalRouter`) | 메타 라우터 진입 |

## 다루는 함수 (SwapRouter / SwapRouter02)

| selector | 시그니처 | mode |
|---|---|---|
| `0x04e45aaf` | `exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))` | ExactIn (single) |
| `0xb858183f` | `exactInput((bytes,address,uint256,uint256,uint256))` | ExactIn (multi, encoded path) |
| `0x5023b4df` | `exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))` | ExactOut (single) |
| `0x09b81346` | `exactOutput((bytes,address,uint256,uint256,uint256))` | ExactOut (multi) |

## Extension `data` 필드

```jsonc
{
  "namespace": "uniswap.v3",
  "data": {
    "feeTier": 500,                         // hundredths of a bip; 500 = 0.05%
    "encodedPath": "0xa0b8...500...c02a",   // 20+(20+3)*k bytes
    "feeTiers": [500, 3000],                // 멀티홉인 경우 hop별 fee
    "sqrtPriceLimitX96": "0",               // uint160 decimal string; 0 = 비활성
    "entryPoint": "SwapRouter02"            // SwapRouter | SwapRouter02 | UniversalRouter
  }
}
```

## 참고사항

- **encoded path**: `tokenIn(20B) + fee(3B) + token(20B) + fee(3B) + tokenOut(20B)` 패턴. 첫·끝 20B로 in/out 토큰 추출, 3B씩으로 hop별 fee tier 추출.
- **풀 주소 도출**: `PoolAddress.computeAddress(factory, key)` (CREATE2). calldata에 없음.
- **`feeTier` 값**: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%).
- **fee bps 변환**: `fee_bps = feeTier / 100`. 500 → 5 bps.
- **multicall**: V3 Router02의 `multicall(uint256, bytes[])` 안에 여러 swap이 포함될 수 있음. v0.1에선 multicall은 `RouterPlan`이 아니라 *flatten*해서 처리.
