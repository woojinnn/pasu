# `uniswap.v4` Extension

Uniswap V4 — singleton `PoolManager` + `Hook` 확장. 모든 swap·liquidity 작업이 `unlock` 콜백 안에서 일어남.

## 진입점

| 컨트랙트 | 주소 (mainnet) | 역할 |
|---|---|---|
| PoolManager | (chain별 상이) | singleton — 모든 풀 상태 보관 |
| PositionManager | (chain별 상이) | NFT 포지션 관리 (`modifyLiquidities` — v0.1 외) |
| UniversalRouter | (별도 namespace `uniswap.universalRouter`) | swap 진입점 |

## v0.1 커버리지

V4는 일반적으로 Universal Router를 통해 진입 (opcode `0x10` `V4_SWAP`). 직접 `PoolManager.swap` 호출은 거의 없음.

`V4_SWAP` opcode 입력은 `(actions: bytes, params: bytes[])` 형태이며, `actions` 바이트 각각이 swap·settle·take·wrap·unwrap을 의미.

## 다루는 V4 Action opcode (UR 안)

| opcode | 이름 | 역할 |
|---|---|---|
| `0x06` | SWAP_EXACT_IN_SINGLE | single hop ExactIn |
| `0x07` | SWAP_EXACT_IN | multi hop ExactIn |
| `0x08` | SWAP_EXACT_OUT_SINGLE | single hop ExactOut |
| `0x09` | SWAP_EXACT_OUT | multi hop ExactOut |
| `0x0c` | SETTLE / SETTLE_ALL | currency 잔여 송금 |
| `0x0e` | TAKE / TAKE_ALL | currency 인출 |
| `0x10` | WRAP | ETH 래핑 |
| `0x11` | UNWRAP | ETH 언래핑 |

## Extension `data` 필드

```jsonc
{
  "namespace": "uniswap.v4",
  "data": {
    "poolKey": {
      "currency0": "0x0000...000",          // native ETH면 0x0
      "currency1": "0xC02a...Cc2",           // WETH 등
      "fee": 500,                            // uint24, 0x800000 마스크 후
      "tickSpacing": 10,
      "hooks": "0x0000...000"                // 0x0 = no hook
    },
    "hookData": "0x",                        // hook 호출에 전달
    "hookFlags": ["beforeSwap", "afterSwap"], // hook 주소 마지막 14 bits decode
    "deltas": [...]                          // settle/take 정산 결과 (event-derived)
  }
}
```

## 참고사항

- **PoolId**: `keccak256(abi.encode(poolKey))` — 풀의 식별자, 컨트랙트 주소가 아님.
- **`fee` 마스킹**: 최상위 비트 `0x800000`은 dynamic fee marker. 실제 bps 계산 시 마스크 후 사용.
- **Hook flags**: `hooks` 주소의 하위 14비트가 어떤 hook이 활성화됐는지 식별 (IHooks Permissions). swap 관련만 surface (beforeSwap, afterSwap, beforeSwapReturnsDelta, afterSwapReturnsDelta).
- **Confidence**: `hooks != 0x0`이면 confidence ceiling = `medium` (hook 의미가 외부 컨트랙트에 의존).
