# `aerodrome.v1` Extension

Aerodrome V1 — Base 체인의 Solidly fork. volatile pool과 stable pool이 같은 Router에서 공존.

## 진입점

| 컨트랙트 | 주소 (Base, chain 8453) |
|---|---|
| Router | `0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43` |
| Voter | (별도, v0.1 외) |

## Solidly path 구조

V2와 달리 path entry가 **3-tuple (`from`, `to`, `stable`)**:

```solidity
struct Route {
    address from;
    address to;
    bool stable;
    address factory;  // v1.5+ 변형
}
```

## 주요 함수

| selector | 시그니처 | mode |
|---|---|---|
| `0xcac88ea9` | `swapExactTokensForTokens(uint256, uint256, Route[], address, uint256)` | ExactIn |
| `0x...` | `swapExactETHForTokens(...)` payable | ExactIn (입력=ETH) |
| `0x...` | `swapExactTokensForETH(...)` | ExactIn (출력=ETH) |

(전체 selector 목록은 `function-inventory`로 보완 — v0.1에선 핵심만)

## Extension `data` 필드

```jsonc
{
  "namespace": "aerodrome.v1",
  "data": {
    "stable": [false, true],            // path entry별 stable bool 배열
    "factories": ["0x...", "0x..."],     // path entry별 factory (v1.5+)
    "supportingFeeOnTransfer": false
  }
}
```

## fee 정책

| pool 종류 | fee bps |
|---|---|
| volatile | 30 (0.3%) |
| stable | 1 (0.01%) |

`SwapFields.max_fee_bps`는 path 안의 *최대*값 — `volatile`이 하나라도 있으면 30.

## 참고사항

- **파생 풀 주소**: V2처럼 calldata에 없음 — `pairFor(factory, tokenA, tokenB, stable)`로 도출.
- **multi-hop** 가능: path 길이 ≥ 2.
- **Aerodrome V2 (Slipstream)**은 별도 namespace `aerodrome.slipstream`.
