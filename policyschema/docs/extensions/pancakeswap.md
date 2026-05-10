# `pancakeswap` Extension (통합)

BNB Chain·Base·기타 체인의 종합 DEX. **5 component 통합 namespace**: `data.component`로 sub-component 식별.

| `data.component` | 설명 |
|---|---|
| `v2` | Uniswap V2 fork (selector 동일, ABI 동일) |
| `v3` | Uniswap V3 fork — callback이 `pancakeV3SwapCallback`인 점만 차이 |
| `smartRouter` | V2 + V3 + Stable swap을 `multicall(bytes[])`로 묶어 실행 |
| `universalRouter` | Uniswap UR의 PancakeSwap fork — **opcode mask `& 0x3f`** (다른 opcode 공간) |
| `infinity` | CL (concentrated) + BIN (LBP) 풀 — UR opcode `0x10` `INFI_SWAP`로 진입 |

## 주요 진입점 (mainnet, BSC)

| Component | 컨트랙트 |
|---|---|
| v2 Router | `0x10ED43C718714eb63d5aA57B78B54704E256024E` |
| v3 SwapRouter | `0x13f4EA83D0bd40E75C8222255bc855a974568Dd4` |
| smartRouter | `0x13f4EA83D0bd40E75C8222255bc855a974568Dd4` (다른 진입) |
| universalRouter | `0x1A0A18AC4BECDDbd6389559687d1A73d8927E416` |
| infinity Vault | (체인별 상이) |

## Component별 `data` 필드

### `component: "v2"`
```jsonc
{ "component": "v2", "path": ["0x...", "0x..."], "supportingFeeOnTransfer": false }
```

### `component: "v3"`
```jsonc
{
  "component": "v3",
  "feeTier": 2500,                  // hundredths of a bip
  "encodedPath": "0x...",
  "feeTiers": [2500],
  "sqrtPriceLimitX96": "0",
  "entryPoint": "SwapRouter"
}
```

### `component: "smartRouter"`
```jsonc
{
  "component": "smartRouter",
  "branches": [                     // multicall 안의 sub-swap들
    { "kind": "v2", "path": [...] },
    { "kind": "v3", "feeTier": 500, "encodedPath": "0x..." },
    { "kind": "stable", "poolAddress": "0x..." }
  ]
}
```

### `component: "universalRouter"`
```jsonc
{
  "component": "universalRouter",
  "commandsHex": "0x10",
  "mask": 63,                        // 0x3f — Uniswap UR과 다름!
  "commands": [
    { "opcode": 16, "name": "INFI_SWAP", ... }
  ]
}
```

### `component: "infinity"`
```jsonc
{
  "component": "infinity",
  "poolKey": {
    "currency0": "0x...",
    "currency1": "0x...",
    "hooks": "0x...",
    "poolManager": "0x...",          // CL vs BIN 구분
    "fee": 500,
    "parameters": "0x...32B"          // tickSpacing(CL) 또는 binStep(BIN)
  }
}
```

## opcode 마스킹 규칙 (PancakeSwap UR)

| Mask | `& 0x3f` |
|---|---|
| 비트 7 의미 | 별도 의미 없음 (Uniswap UR과 다름!) |

**디코딩 첫 단계는 family 판별** — `tx.to`가 PancakeSwap UR이면 mask `0x3f` 적용.

## 주요 PancakeSwap UR opcode (mask 후)

| opcode | 이름 | 비고 |
|---|---|---|
| `0x00` | V3_SWAP_EXACT_IN | Uniswap UR과 동일 |
| `0x08` | V2_SWAP_EXACT_IN | 동일 |
| `0x10` | INFI_SWAP | Infinity (CL+BIN) 진입 |
| `0x22` | STABLE_SWAP_EXACT_IN | Curve-style stable swap |
| `0x23` | STABLE_SWAP_EXACT_OUT | 동상 |

## 참고사항

- **callback name 차이**: V3 fork이지만 callback이 `pancakeV3SwapCallback` (Uniswap은 `uniswapV3SwapCallback`). 디코더가 family 판별.
- **Infinity PoolKey 6필드**: 일반 V4 PoolKey(5필드)와 다르게 `poolManager` 필드가 있음 (CL vs BIN 구분).
- **`parameters: bytes32`**: Infinity는 `tickSpacing` (CL) 또는 `binStep` (BIN)을 32바이트 packed로 인코딩. v0.1에서는 raw bytes로 보존.
