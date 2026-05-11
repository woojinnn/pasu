# `balancer.v3` Extension

Balancer V3 — 2024년 출시. V2 Vault 재설계 + 새 라우터 컨트랙트 + Hook 시스템.

**통합 namespace** (`pancakeswap`/`lido` 패턴) — `data.component`로 진입점 식별:
- `vault` — V3 Vault (single swap, addLiquidity, removeLiquidity)
- `batchRouter` — BatchRouter (멀티홉 swap) ⭐
- `compositeRouter` — CompositeRouter (Boosted Pool wrap+swap 자동) — v0.2

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| V3 Vault | `0xbA1333333333a1BA1108E8412f11850A5C319bA9` |
| V3 BatchRouter | `0x136f1efcc3f8f88516b9e94110d56fdbfb1778d1` |
| V3 CompositeRouter | (v0.2) |

## V2와의 핵심 차이

| 항목 | V2 | V3 |
|---|---|---|
| 풀 식별 | `bytes32 poolId` | `address pool` 직접 |
| Hook | ❌ | ✅ (Uniswap V4 영향) |
| batchSwap 위치 | Vault | **BatchRouter** (별도 컨트랙트) ⭐ |
| 새 풀 타입 | Weighted, Stable, Composable Stable | + Custom AMM, **Boosted Pool**, **Buffer Pool** |
| IAsset sentinel | `0x0` = ETH | (제거, `wethIsEth` 플래그로 대체) |

## V3 Vault 함수

| selector | 시그니처 | ActionType |
|---|---|---|
| `0x2bfaa459` | `swap(VaultSwapParams params)` | Swap |
| `0x5549a3b0` | `addLiquidity(AddLiquidityParams params)` | JoinPool |
| `0xab5549a3` | `removeLiquidity(RemoveLiquidityParams params)` | ExitPool |

### VaultSwapParams

```solidity
struct VaultSwapParams {
    SwapKind kind;           // 0 = EXACT_IN, 1 = EXACT_OUT
    address pool;             // V2의 bytes32 poolId 대신
    IERC20 tokenIn;
    IERC20 tokenOut;
    uint256 amountGivenRaw;   // kind에 따라 in/out exact
    uint256 limitRaw;         // 반대편 한도
    bytes userData;
}
```

## V3 BatchRouter 함수 (멀티홉) ⭐

| selector | 시그니처 | ActionType |
|---|---|---|
| `0x286f580d` | `swapExactIn(SwapPathExactAmountIn[] paths, uint256 deadline, bool wethIsEth, bytes userData)` | BatchSwap |
| `0x9a99b4f0` | `swapExactOut(SwapPathExactAmountOut[] paths, uint256 deadline, bool wethIsEth, bytes userData)` | BatchSwap |

### SwapPath 구조

```solidity
struct SwapPathExactAmountIn {
    IERC20 tokenIn;
    SwapPathStep[] steps;       // N step = N hop ⭐
    uint256 exactAmountIn;
    uint256 minAmountOut;
}

struct SwapPathStep {
    address pool;
    IERC20 tokenOut;
    bool isBuffer;               // V3 신규 — buffer pool (wrap-unwrap)
}
```

### 매핑 규칙

| 입력 모양 | `SwapRoute` variant | 의미 |
|---|---|---|
| `paths.length == 1` + `steps.length == 1` | `SingleHop` | 단일 풀 |
| `paths.length == 1` + `steps.length > 1` | `MultiHop` | 선형 멀티홉 |
| `paths.length > 1` | `Split` | 다중 path를 branches로 flatten |
| (중첩) | (현재 평면화 — v0.2 그래프) | nested split-of-multihop |

### Buffer Pool

`SwapPathStep.isBuffer = true`이면 hop의 `protocol = "balancer.v3.buffer"`로 표기 (wrap/unwrap 의미).

## Extension `data` 필드

```jsonc
// V3 Vault
{
  "namespace": "balancer.v3",
  "data": {
    "component": "vault",
    "userData": "0x..."
  }
}

// V3 BatchRouter
{
  "namespace": "balancer.v3",
  "data": {
    "component": "batchRouter",
    "pathsCount": 1,
    "stepsTotal": 2,
    "wethIsEth": false
  }
}
```

## Hook 시스템 (개요)

V3는 Uniswap V4처럼 풀에 hook을 부착 가능. v0.1에서는 hook flag 존재 여부만 표시 (`HookedOperation` ActionType 활용). 세부 hook 의미론은 v0.2 — 풀별 hook 주소의 권한 비트를 디코딩.

## Out-of-scope (v0.2 후보)

- CompositeRouter (Boosted Pool wrap+swap 자동 조합)
- Boosted Pool yield 분리 (linear pool wrapped → underlying)
- Buffer Pool 정확한 conversion rate decoding
- 다중 path × 다중 step nested routing의 *그래프* 표현 (현재 평면화)
