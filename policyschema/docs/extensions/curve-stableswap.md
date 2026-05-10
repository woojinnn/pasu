# `curve.stableswap` Extension

Curve StableSwap pool — `tokens i, j` 인덱스로 swap.

## 핵심 함수

```solidity
function exchange(int128 i, int128 j, uint256 dx, uint256 min_dy) external returns (uint256);
function exchange_underlying(int128 i, int128 j, uint256 dx, uint256 min_dy) external returns (uint256);
```

## Extension `data` 필드

```jsonc
{
  "namespace": "curve.stableswap",
  "data": {
    "poolAddress": "0x...",
    "i": 0,                      // 입력 토큰 풀 인덱스
    "j": 1,                      // 출력 토큰 풀 인덱스
    "isUnderlying": false,        // exchange_underlying이면 true
    "feeBps": 4                   // 풀별 fee (기본 0.04% = 4 bps)
  }
}
```

## 참고

- **`i`/`j` 인덱스**: 풀의 `coins[]` 배열 위치. 토큰 주소 도출은 풀 컨트랙트 view 호출 필요.
- **`exchange_underlying`**: lending pool용 — underlying token 직접 사용 (예: aDAI 풀의 DAI).
- v0.1에서는 *세미-어댑터 미구현* — 데이터 모델만.
