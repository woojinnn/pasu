# DEX 계열 Action 가이드 — swap / 4-way liquidity / 2-way NFT liquidity

본 문서는 DEX 맥락에서 가장 자주 등장하는 7 가지 action 의 schema 를 사용자/정책 작성자 관점에서 풀어 설명합니다.

- **swap** — 토큰 교환
- **add_liquidity / remove_liquidity** — fungible LP/BPT 발행 및 소각 (V2, Curve, Balancer)
- **mint_liquidity_nft / burn_liquidity_nft** — V3/V4 NFT position 발행 및 소각
- **increase_liquidity / decrease_liquidity** — V3/V4 NFT 의 internal liquidity 변화

---

## 0. 시작하기 전에 — action 과 category 는 직교 차원

v101 에서는 action 이 category 에 종속되지 않습니다. swap/add_liquidity/remove_liquidity 등이 무조건 `category=dex` 인 것은 *아닙니다* — DEX 외 category 에서도 같은 의미 단위가 등장합니다:

| action | 가장 흔한 category | 다른 category 에 등장하는 케이스 |
|---|---|---|
| `swap` | `dex` | `liquid_staking` (Lido stETH ↔ wstETH), `rwa` (LBTC ↔ WBTC 1:1 minting), `lending` (일부 lending 의 collateral swap routing) |
| `add_liquidity` | `dex` | `yield` (vault deposit) |
| `remove_liquidity` | `dex` | `yield` (vault withdraw) |
| `mint/burn/increase/decrease_liquidity` | `dex` | (현재로서는 V3/V4 NFT 와 거의 동의) |

실제 발생 맥락은 root level 에서 결정. 정책은 `action=swap AND category=dex` 같은 결합 분기 가능.

---

## 1. liquidity action 의 6-way 분리 (자산 변화 형태별)

DEX 의 liquidity 동작은 **wallet 의 자산이 어떻게 변하는가** 에 따라 의미가 다릅니다. 그래서 6개 action 으로 분리:

| Action | wallet 변화 | 적용 protocol |
|---|---|---|
| `add_liquidity` | ERC-20 LP/BPT 잔액 ↑ | V2 / PancakeSwap V2 / Aerodrome / Sushi V2 / Curve V1·NG·Crypto / Balancer V2·V3 |
| `remove_liquidity` | ERC-20 LP/BPT 잔액 ↓, underlying 토큰 ↑ | 위 동일 |
| `mint_liquidity_nft` | NFT 보유 수 +1, token0/token1 ↓ | Uniswap V3 NPM / V4 PositionManager |
| `burn_liquidity_nft` | NFT 보유 수 -1 (+ V4 의 경우 token 회수) | 위 동일 |
| `increase_liquidity` | NFT 의 internal state 변화 (보유 수 불변, token 잔액 ↓) | 위 동일 |
| `decrease_liquidity` | NFT 의 internal state 변화 (V3 는 tokensOwed 적립만, V4 는 token 회수 atomic) | 위 동일 |

이 분리는 wallet UI 가 사용자에게 보여줄 변화 시나리오와 정책 검사 방향이 각각 다르기 때문에 의미가 있습니다.

### 1.1 AmountConstraint 미리 알기

DEX action 의 거의 모든 amount 가 이 wrapper 를 씁니다:

```jsonc
{ "kind": "exact"|"min"|"max"|"unlimited"|"estimated"|"unknown", "value": "1000000" }
```

| kind | DEX 에서의 흔한 등장 |
|---|---|
| `exact` | swap 의 amountIn (exact_in), Curve add_liquidity 의 amounts[N], V2 removeLiquidity 의 liquidity |
| `min` | swap 의 amountOutMin, add_liquidity 의 minAmountsIn, remove 의 minAmountsOut (모두 슬리피지 하한) |
| `max` | exact_out swap 의 amountInMax, Balancer joinPool 의 maxAmountsIn |
| `estimated` | V2/V3 NPM 의 amountDesired (라우터가 quote 한 추정치) |
| `unknown` | V4 PoolManager.swap 처럼 minOutput 보장이 calldata 에 없는 경우 |
| `unlimited` | DEX 에서 거의 안 쓰임 (approve 에서 주로) |

---

## 2. `swap` — 토큰 교환

### 2.1 어떤 함수들이

매우 다양 — 본 schema 가 이걸 1개로 통합한 가치가 가장 큽니다.

| Protocol | 대표 함수 |
|---|---|
| Uniswap V2 (+ Pancake V2) | `swapExactTokensForTokens` / `swapTokensForExactTokens` / `swapExactETHForTokens` 등 9 변형 |
| Uniswap V3 SwapRouter / SwapRouter02 | `exactInputSingle` / `exactInput` / `exactOutputSingle` / `exactOutput` (deadline 변형 포함 8개) |
| Uniswap V4 | `PoolManager.swap` + V4Router opcode 0x06~0x09 |
| Universal Router | execute 의 opcode `0x00` (V3_SWAP_EXACT_IN), `0x08` (V2_SWAP_EXACT_IN), `0x10` (V4_SWAP) 등 |
| Balancer V2/V3 | `Vault.swap` (single), `Vault.batchSwap` |
| Curve | `exchange` (int128 / uint256 변형), `exchange_underlying`, `exchange_received`, CurveRouter.exchange |
| Aerodrome / Sushi / Maverick / Trader Joe LB | 각자의 swap 함수들 |
| Pancake SmartRouter / Infinity | `exactInputSingle`, `exactInputStableSwap`, `cl.swap`, `bin.swap` 등 |

→ **약 50+ 컨트랙트 함수가 모두 본 schema 하나로** 정규화됩니다.

### 2.2 필드

```jsonc
{
  "mode":       "exact_in",                                          // 또는 exact_out / market / unknown
  "tokenIn":    { "kind": "erc20", "address": "0xA0b8…", "symbol": "USDC", "decimals": 6 },
  "tokenOut":   { "kind": "erc20", "address": "0xC02a…", "symbol": "WETH", "decimals": 18 },
  "amountIn":   { "kind": "exact", "value": "1000000000" },          // 1,000 USDC
  "amountOut":  { "kind": "min",   "value": "300000000000000000" },  // 최소 0.3 WETH
  "recipient":  "0xUser…",
  "slippageBps": 50,                                                  // 0.5%
  "validity":   { "expiresAt": "1715961834", "source": "tx-deadline" },
  "feeBps":     5                                                     // V3 0.05% pool
}
```

| 필드 | 정책 관점 |
|---|---|
| **`mode`** | exact_in / exact_out / market / unknown. 정책 'market 또는 exact_out 차단' 분기 — market 은 슬리피지 보호 없는 무방어 호출 |
| **`tokenIn` / `tokenOut`** | 가장 흔한 정책 분기 자리. "USDC 와 WETH 만 허용" / "no MEV bait tokens" / "stablecoin → stablecoin 만" |
| `amountIn` / `amountOut` | mode 와 함께 보면 의미 명확 |
| **`recipient`** (required) | swap 결과 수령자. **`root.from` 과 다르면 phishing 1차 신호.** 정책 강력 권장: `recipient == root.from` |
| **`slippageBps`** | 슬리피지 허용. 정책 "max 3% (300 bps)" — sandwich attack 방지 |
| `validity` | `source="tx-deadline"` — tx 가 이 시점 전에 mining 되어야 함. 정책 "(expiresAt - blockTimestamp) 가 60s ~ 1h 범위". V4 PoolManager.swap 등 deadline 없는 함수는 omit |
| `feeBps` | pool 의 swap fee. "0.30% (30 bps) 이상 pool 차단" 같은 분기 (low-fee blue chip 만 허용) |

> **USD 환산값은 schema 표면에 없습니다.** Oracle 데이터는 schema 인스턴스가 만들어진 *이후* 별도 enrichment 단계에서 attach 됩니다.

### 2.3 정책 예시

```text
// 1. 가장 흔한 안전판
recipient == from
slippageBps <= 300
validity == null || (validity.expiresAt - root.blockTimestamp) between 60 and 1800

// 2. 토큰 허용 목록
tokenIn.symbol in ["USDC","USDT","WETH","DAI"]
tokenOut.symbol in ["USDC","USDT","WETH","DAI"]

// 3. 무방어 호출 차단
mode not in ["exact_out", "market"]

// 4. raw amount 상한 (USD enrichment 전)
amountIn.value <= "1000000000"   // 예: USDC 1,000 단위 (6 decimals)
```

---

## 3. `add_liquidity` — fungible LP 발행 (V2/Curve/Balancer)

V3/V4 NFT 발행은 §5 mint_liquidity_nft 참조.

### 3.1 어떤 함수들이

| Protocol | 대표 함수 |
|---|---|
| Uniswap V2 / Pancake V2 / Aerodrome V2 / Sushi V2 | `addLiquidity(tokenA, tokenB, …)`, `addLiquidityETH(token, …)` |
| Curve V1 / NG / Crypto | `add_liquidity(uint256[N] amounts, uint256 min_mint_amount)` (N=2~4) |
| Balancer V2 / V3 | `Vault.joinPool(poolId, sender, recipient, request)` |

### 3.2 필드

```jsonc
{
  "pool": {
    "address": "0x…",
    "id":      "0x…",                                  // Balancer 한정
    "label":   "ETH/USDC (Curve)"                      // host:registry
  },
  "tokens": [ AssetRef_USDC, AssetRef_WETH ],
  "amounts": [
    { "kind": "estimated", "value": "1000000000" },     // ~1,000 USDC desired
    { "kind": "estimated", "value": "300000000000000000" }
  ],
  "minAmountsIn": [                                     // V2 amountAMin/amountBMin
    { "kind": "min", "value": "990000000" },
    { "kind": "min", "value": "297000000000000000" }
  ],
  "lpToken": { "kind": "erc20", "address": "0x…", "symbol": "UNI-V2", "decimals": 18 },
  "lpAmount": { "kind": "min", "value": "1000000000000000000" },  // Curve min_mint_amount
  "recipient": "0xUser…",
  "validity":  { "expiresAt": "1715961834", "source": "tx-deadline" }
}
```

| 필드 | 정책 관점 |
|---|---|
| **`pool.label`** | host:registry 가 채우는 사람-친화적 라벨. "Unknown pool" 이면 정책상 차단 권장 |
| **`tokens` / `amounts`** | 어느 토큰을 얼마나 넣는가. 정책 "stablecoin pool 만" |
| **`minAmountsIn`** | V2 의 토큰별 슬리피지 보호. Curve/Balancer 는 LP 측 만 가지므로 본 필드 생략. 모든 entry value=0 이면 슬리피지 보호 없음 — 정책 강력 차단 |
| **`lpToken`** | 발행되는 LP / BPT 의 AssetRef. 정책 "whitelisted LP token 만" |
| **`lpAmount`** | LP 측 슬리피지 보호 — Curve `min_mint_amount`, Balancer `minimumBPT` 등. kind=min 보장 |
| `recipient` (required) | LP 수령자. `from` 과 다르면 비정상 |
| `validity` | source='tx-deadline' (Curve 는 deadline 부재 — omit) |

### 3.3 정책 예시

```text
// 1. 안전판
recipient == from
(minAmountsIn != null && exists i: minAmountsIn[i].value > "0") ||
  (lpAmount != null && lpAmount.kind in ["min", "exact"])   // 슬리피지 보호 존재
validity == null || (validity.expiresAt - root.blockTimestamp) >= 60

// 2. pool whitelist
pool.label != null
pool.label matches "(WETH|USDC|USDT|DAI)/.*"

// 3. lpToken whitelist
lpToken != null && lpToken.symbol matches "(UNI-V2|crv|BPT)-.*"

// 4. 토큰 화이트리스트
forall t in tokens: t.symbol in ["USDC","USDT","WETH","DAI"]
```

---

## 4. `remove_liquidity` — fungible LP 소각 (V2/Curve/Balancer)

V3/V4 NFT 의 internal liquidity 감소는 §7 decrease_liquidity, NFT 소각은 §6 burn_liquidity_nft 참조.

### 4.1 어떤 함수들이

| Protocol | 대표 함수 |
|---|---|
| V2 / Pancake V2 / Aerodrome / Sushi V2 | `removeLiquidity` / `removeLiquidityETH` / `…WithPermit` / FOT 변형 등 6개 |
| Curve V1 / NG / Crypto | `remove_liquidity` (proportional), `remove_liquidity_imbalance` (exact_out), `remove_liquidity_one_coin` (single_asset) |
| Balancer V2 / V3 | `Vault.exitPool` (V2), `removeLiquidity*` 4 변형 (V3) |

### 4.2 필드

```jsonc
{
  "exitMode":      "proportional",                       // 또는 single_asset / exact_out
  "pool":          { "address": "0x…", "label": "3pool (Curve)" },
  "lpToken":       { "kind": "erc20", "address": "0x…", "symbol": "3CRV", "decimals": 18 },
  "lpBurnAmount":  { "kind": "exact", "value": "500000000000000000" },
  "tokens":        [ AssetRef_DAI, AssetRef_USDC, AssetRef_USDT ],
  "minAmountsOut": [
    { "kind": "min", "value": "165000000000000000000" }, // 최소 165 DAI
    { "kind": "min", "value": "165000000" },             // 최소 165 USDC
    { "kind": "min", "value": "165000000" }              // 최소 165 USDT
  ],
  "recipient":     "0xUser…",
  "validity":      { "expiresAt": "1715961834", "source": "tx-deadline" }
}
```

| 필드 | 정책 관점 |
|---|---|
| **`exitMode`** | 3 종. proportional (V2 / Curve / Balancer EXACT_BPT_IN_FOR_TOKENS_OUT), single_asset (Curve one_coin / Balancer EXACT_BPT_IN_FOR_ONE_TOKEN_OUT), exact_out (Curve imbalance / Balancer BPT_IN_FOR_EXACT_TOKENS_OUT) |
| **`lpToken`** | 소각되는 LP 토큰의 AssetRef |
| **`lpBurnAmount`** | 소각 양. kind=exact (대부분) 또는 kind=max (Curve imbalance, Balancer SingleTokenExactOut) |
| `tokens` / **`minAmountsOut`** | 받을 토큰과 최소 보장. 모든 entry 의 kind=min, value=0 이면 sandwich attack 100% 노출 — 정책 강력 차단 |
| **`recipient`** (required) | ★ 가장 위험한 필드. underlying 토큰 (보통 가치 큰 자산) 수령자. `from` 과 다르면 account drain 패턴 — 정책 무조건 `recipient == from` |
| `validity` | source='tx-deadline'. Curve 는 deadline 부재 — omit |

### 4.3 정책 예시

```text
// 1. 강력한 안전판
recipient == from                  // 절대 sine qua non
validity == null || (validity.expiresAt - root.blockTimestamp) >= 60
forall i in minAmountsOut: i.value > "0"   // 모든 토큰에 하한 있어야

// 2. exit mode 제한
exitMode in ["proportional"]   // imbalance / single_asset 차단

// 3. lpToken whitelist
lpToken.symbol in ["3CRV","UNI-V2","BPT-…"]
```

---

## 5. `mint_liquidity_nft` — V3/V4 신규 NFT position 발행

### 5.1 어떤 함수들이

| Protocol | 대표 함수 |
|---|---|
| Uniswap V3 NonfungiblePositionManager | `mint(MintParams)` |
| Uniswap V4 PositionManager | `modifyLiquidities` 의 MINT_POSITION action (0x02) |
| PancakeSwap V3 / Pancake Infinity CL | 동상 (NPM fork) |

### 5.2 필드

```jsonc
{
  "pool": {
    "address": "0x88e6…",                                    // (V3) PoolAddress.computeAddress(token0,token1,fee) 결과
    "label":   "USDC/WETH 0.05% (V3)"                        // host:registry
  },
  "feeTierBps": 5,                                           // V3 0.05% tier
  "tickRange": { "lower": -200000, "upper": 200000 },
  "tokens": [ AssetRef_USDC_token0, AssetRef_WETH_token1 ],  // lexicographic 정렬
  "amounts": [
    { "kind": "estimated", "value": "1000000000" },
    { "kind": "estimated", "value": "300000000000000000" }
  ],
  "minAmountsIn": [
    { "kind": "min", "value": "990000000" },
    { "kind": "min", "value": "297000000000000000" }
  ],
  "nft": { "kind": "erc721", "address": "0xC36442b4…" },     // V3 NPM 컨트랙트
  "recipient": "0xUser…",
  "validity":  { "expiresAt": "1715961834", "source": "tx-deadline" }
}
```

| 필드 | 정책 관점 |
|---|---|
| **`pool`** | add_liquidity / remove_liquidity 와 동일 형식 (`{address?, id?, label?}`). pool 식별은 host:registry 가 채우는 label 로 사용자 친화적 분기 |
| **`feeTierBps`** | V3/V4 pool fee tier (basis point). 100/500/3000/10000 등. dynamic-fee (V4) 는 `0x800000` bit set — 정책 'dynamic 차단' 분기 가능 |
| **`tickRange`** | concentrated liquidity 범위. 정책 'tickUpper - tickLower >= N' (out-of-range 위험 차단) |
| `tokens` / `amounts` / `minAmountsIn` | 토큰별 desired + 슬리피지 하한 (V3 amount0/1Min) |
| **`nft`** | 발행 NFT collection. 정책 "공인 NPM 만" |
| **`recipient`** (required) | 새 NFT 의 owner. `from` 과 다르면 제3자 발행 — 의심 |

### 5.3 정책 예시

```text
recipient == from
feeTierBps in [100, 500, 3000]
tickRange.upper - tickRange.lower >= 1000   // 너무 좁은 범위 차단
forall t in tokens: t.symbol in ["USDC","USDT","WETH","DAI"]
nft.address in [V3_NPM_MAINNET, V4_POSMGR_MAINNET]
```

---

## 6. `burn_liquidity_nft` — V3/V4 NFT 소각

V3 와 V4 의 burn 동작 의미가 완전히 다르므로 `burnKind` 로 구분.

### 6.1 어떤 함수들이

| 종류 | 함수 | 의미 |
|---|---|---|
| `empty_only` | V3 `NPM.burn(tokenId)` | 선결조건 (liquidity==0 && tokensOwed==0) 만족된 빈 NFT 정리, 자금 이동 없음 |
| `auto_decrease` | V4 `modifyLiquidities` 의 BURN_POSITION action (0x03) | 전체 liquidity 감소 + token take + NFT burn atomic, recipient 로 자금 송금 |

### 6.2 필드

```jsonc
// empty_only (V3) — 자금 이동 없음
{
  "nft":      { "kind": "erc721", "address": "0xC36442b4…" },
  "tokenId":  "12345",
  "burnKind": "empty_only"
}

// auto_decrease (V4) — 자금 이동 있음
{
  "nft":           { "kind": "erc721", "address": "0xbd216513d7…" },
  "tokenId":       "67890",
  "burnKind":      "auto_decrease",
  "outputs":       [ AssetRef_token0, AssetRef_token1 ],
  "minAmountsOut": [{ "kind": "min", "value": "990000000" }, { "kind": "min", "value": "297000000000000000" }],
  "recipient":     "0xUser…",
  "validity":      { "expiresAt": "1715961834", "source": "tx-deadline" }
}
```

| 필드 | 정책 관점 |
|---|---|
| **`burnKind`** | 정책 'auto_decrease 차단 또는 엄격 검사'. empty_only 는 거의 통과, auto_decrease 는 recipient/slippage 검증 필수 |
| **`recipient`** | burnKind=auto_decrease 시 토큰 수령자. account drain 패턴 차단 |
| **`minAmountsOut`** | auto_decrease 시 슬리피지 보호 |

---

## 7. `increase_liquidity` / `decrease_liquidity` — V3/V4 기존 NFT 의 internal liquidity 변화

### 7.1 어떤 함수들이

| Action | 함수 |
|---|---|
| `increase_liquidity` | V3 `NPM.increaseLiquidity(IncreaseLiquidityParams)`, V4 `modifyLiquidities` 의 INCREASE_LIQUIDITY action (0x00) |
| `decrease_liquidity` | V3 `NPM.decreaseLiquidity(DecreaseLiquidityParams)`, V4 `modifyLiquidities` 의 DECREASE_LIQUIDITY action (0x01) |

### 7.2 increase_liquidity 필드

```jsonc
{
  "nft":     { "kind": "erc721", "address": "0xC36442b4…" },
  "tokenId": "12345",
  "tokens":  [ AssetRef_token0, AssetRef_token1 ],         // V3 는 host:onchain 보강
  "amounts": [
    { "kind": "estimated", "value": "1000000000" },
    { "kind": "estimated", "value": "300000000000000000" }
  ],
  "minAmountsIn": [
    { "kind": "min", "value": "990000000" },
    { "kind": "min", "value": "297000000000000000" }
  ],
  "validity": { "expiresAt": "1715961834", "source": "tx-deadline" }
}
```

| 필드 | 정책 관점 |
|---|---|
| **`tokenId`** | 기존 position id. 정책 'host:onchain 으로 NFT.ownerOf(id) == from 검증' |
| `tokens` / `amounts` / `minAmountsIn` | 토큰별 예치 + 슬리피지 (mint 와 동일) |

→ V3 는 calldata 에 토큰 주소 없으므로 host:onchain 보강 (NFT.positions(tokenId) 조회).

### 7.3 decrease_liquidity 필드

```jsonc
{
  "nft":            { "kind": "erc721", "address": "0xC36442b4…" },
  "tokenId":        "12345",
  "liquidityDelta": { "kind": "exact", "value": "100000000000" },  // 감소량 (uint128, 절대값)
  "outputs":        [ AssetRef_token0, AssetRef_token1 ],
  "minAmountsOut": [
    { "kind": "min", "value": "490000000" },
    { "kind": "min", "value": "147000000000000000" }
  ],
  "validity": { "expiresAt": "1715961834", "source": "tx-deadline" }
}
```

| 필드 | 정책 관점 |
|---|---|
| **`liquidityDelta`** | 감소시킬 internal liquidity 양 (uint128). 정책 "한 번에 X 이상 감소 차단" |
| **`minAmountsOut`** | 슬리피지 보호. V3 = tokensOwed 적립, V4 = recipient 송금 |

→ V3 는 wallet 자산 변화 없음 (tokensOwed 적립만). V4 는 같은 action 안에서 TAKE 까지 묶여 wallet 으로 송금.

### 7.4 정책 예시

```text
// V3/V4 NFT 공통
nft.address in [V3_NPM_MAINNET, V4_POSMGR_MAINNET]   // 공인 컨트랙트만

// increase_liquidity 안전판
validity == null || (validity.expiresAt - root.blockTimestamp) >= 60
forall i in minAmountsIn: i.value > "0"

// decrease_liquidity 안전판
forall i in minAmountsOut: i.value > "0"
liquidityDelta.kind == "exact"   // unknown 차단
```

---

## 8. action 분류 빠른 참조표

| wallet 변화 | action | 보유 NFT 변화 | 자금 이동 |
|---|---|---|---|
| fungible LP/BPT 잔액 증가 | `add_liquidity` | — | tokens → pool |
| fungible LP/BPT 잔액 감소 | `remove_liquidity` | — | pool → recipient |
| NFT 보유 +1 | `mint_liquidity_nft` | +1 | tokens → pool |
| NFT 보유 -1 (V3 empty_only) | `burn_liquidity_nft` | -1 | (없음) |
| NFT 보유 -1 (V4 auto_decrease) | `burn_liquidity_nft` | -1 | pool → recipient |
| 잔액 불변, NFT internal ↑ | `increase_liquidity` | 0 | tokens → pool |
| 잔액 불변 또는 ↑ (V4), NFT internal ↓ | `decrease_liquidity` | 0 | (V3) pool → tokensOwed / (V4) pool → recipient |

---

## 9. 본 schema 가 통합하지 *못 하는* 것

| 케이스 | 처리 |
|---|---|
| V4 hook 의 swap-의미 변경 (beforeSwap 이 amount 변형) | hook 의 의미 디코드는 v1.0.1 범위 외. 향후 Extension Schema |
| Curve `exchange_received` (pre-funded swap, 사용자가 직접 transferFrom 안 함) | swap action 으로 정규화하되 settlement 의미 차이는 Extension Schema |
| Balancer batchSwap 의 multi-asset chaining | swap action 1개로 통합 (첫 토큰 in / 마지막 토큰 out 만 보존) |
| Pancake StableSwap 의 `flag[i]` | 라우팅 디테일 — schema 표면 외 |

---

## 10. 자주 묻는 질문

**Q. V3/V4 의 collectFees 는 어떤 action 인가?**
A. 현재 v1.0.1 에는 없음. tokensOwed 회수만 하는 별도 동작이라 misc/transfer 또는 별도 action 후보. 향후 검토.

**Q. Curve `remove_liquidity_one_coin` 의 `tokens` 길이는?**
A. pool 전체 N 개 보존. `minAmountsOut[i].value > 0` 인 entry 가 실제 받을 토큰.

**Q. Balancer `batchSwap` 은 swap 1개?**
A. 1개 swap action. multi-hop chaining 이라도 사용자 의미는 "tokenIn → tokenOut 1회".

**Q. mint_liquidity_nft 의 mintedTokenId 가 안 보이는데?**
A. tx 직후 발급되는 tokenId 는 post-execution 정보 (event log) — schema 표면 외. host 의 별도 enrichment 단계에서 채움.

---

## 11. 관련 파일

- `schema/actions/swap.json`
- `schema/actions/add_liquidity.json` / `remove_liquidity.json`
- `schema/actions/mint_liquidity_nft.json` / `burn_liquidity_nft.json`
- `schema/actions/increase_liquidity.json` / `decrease_liquidity.json`
- `schema/common/_common.json` (AssetRef, AmountConstraint, Validity)
- `docs/root-schema.md` (이들이 어떻게 root.actions[] 에 묶이는가)
- `docs/misc-actions.md` (DEX action 의 prerequisite — approve / wrap)
- `contracts/` — 본 schema 가 정규화 대상으로 삼는 reference 컨트랙트
