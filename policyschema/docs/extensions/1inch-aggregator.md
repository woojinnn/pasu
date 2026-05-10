# `1inch.aggregator` Extension

1inch v5/v6 Aggregation Router — multi-DEX 라우팅.

## 진입점

| 컨트랙트 | 주소 (mainnet, v6) |
|---|---|
| AggregationRouterV6 | `0x111111125421ca6dc452d289314280a0f8842a65` |

## 핵심 함수

```solidity
function swap(IAggregationExecutor executor, SwapDescription desc, bytes permit, bytes data) external payable;
function uniswapV3Swap(uint256 amount, uint256 minReturn, uint256[] pools) external;
function unoswap(address srcToken, uint256 amount, uint256 minReturn, uint256[] pools) external;
```

## Extension `data` 필드

```jsonc
{
  "namespace": "1inch.aggregator",
  "data": {
    "version": "v5" | "v6",
    "executor": "0x...",         // 외부 executor (커스텀 라우팅 로직)
    "swapDescription": { "srcToken": "0x...", "dstToken": "0x...", ... },
    "pools": ["0x...", "0x..."], // packed pool list (v5/v6 인코딩)
    "flags": 0                    // partialFill·shouldClaim 등 플래그 비트
  }
}
```

## 참고

- **executor 위험**: 임의 컨트랙트 콜이 가능 — confidence ceiling = `medium`.
- **packed pools**: 풀 주소 + direction bit + fee 정보가 uint256에 인코딩 — 별도 디코더 필요.
- v0.1 *세미-어댑터 미구현*.
