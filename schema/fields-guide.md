# swap.json / liquidity.json 필드 설명서

본 문서는 `action-fields/swap.json` 와 `action-fields/liquidity.json` 의 모든 필드를 **사용자 입장 (지갑 UI 가 보여줄 만한 의미)** 로 풀어서 설명합니다.

지갑이 받은 한 건의 트랜잭션은 N 개의 `NormalizedAction` 으로 분해됩니다. 그 중 `category = "swap"` 또는 `"liquidity"` 인 action 의 `fields` 가 본 문서의 대상입니다.

---

## 0. 모든 필드의 공통 형식 — `AmountConstraint` 와 `AssetRef`

읽기 전에 두 가지를 먼저 이해해야 합니다. 본 schema 의 거의 모든 금액과 토큰은 이 두 형식을 따릅니다.

### `AmountConstraint` — "이 숫자가 정확한 값인가, 한계값인가"

같은 `1000000` 이라도 **정확히 이만큼인지 / 최소 이만큼 받아야 하는지 / 최대 이만큼만 낼 의향인지** 는 정책상 완전히 다른 의미입니다. 그래서 모든 금액은 다음 객체로:

```jsonc
{ "kind": "exact" | "min" | "max" | "unlimited" | "estimated" | "unknown",
  "value": "1000000" }    // 10진 문자열 (uint256 안전 표현)
```

| kind | 의미 | 사용 예 |
|---|---|---|
| `exact` | "**정확히 이만큼**" | `swapExactTokensFor…` 의 `amountIn`, `exactInput…` 의 `amountIn` |
| `min` | "**최소 이만큼은 받아야**" (슬리피지 하한) | `amountOutMin`, `amountOutMinimum` |
| `max` | "**최대 이만큼까지만 낼 의향**" (슬리피지 상한) | `amountInMax`, `amountInMaximum`, LP 인출 시 `max_burn_amount` |
| `unlimited` | uint256 max — **무한 허용** | 무한 approval, 무한 deadline |
| `estimated` | 라우터가 quote 한 **추정치** (확정 아님) | V2 `addLiquidity` 의 `amountDesired` |
| `unknown` | calldata 만으로는 결정 못 함 | V4 PoolManager.swap 에서 minOutput 부재 |

### `AssetRef` — "어떤 자산인가"

```jsonc
{ "kind": "native" | "erc20" | "erc721" | "erc1155" | "unknown",
  "chainId": 1,
  "address": "0x…",         // erc20/721/1155 일 때 필수
  "symbol":  "USDC",         // 호스트가 토큰 레지스트리로 보강
  "decimals": 6,             // 동상
  "isNative": false }
```

- `kind="native"` = 체인 native (ETH/BNB 등). sentinel `0xeeee…eeee` 로 표기되거나 address 가 omit 될 수 있음.
- `symbol` / `decimals` 가 비어 있을 수 있음 — calldata 만으로는 ERC-20 메타를 알 수 없기 때문. **지갑/서버 호스트가 채워줘야** 함.

---

## 1. `swap.json` — SwapActionFields

`category = "swap"` 인 action 의 `fields` 형식. `_kind ∈ { swap, batch_swap, hooked_operation }` 3 종 모두 본 shape 를 공유합니다.

### 1.1 식별

| 필드 | 의미 | 사용자 관점 |
|---|---|---|
| `_kind` | discriminator. `swap` / `batch_swap` / `hooked_operation` | `swap` = 일반 1-경로 스왑, `batch_swap` = Balancer multi-asset 묶음, `hooked_operation` = V4 hook 이 swap 의미를 바꾸는 경우 |
| `mode` | `exact_in` / `exact_out` / `market` / `unknown` | "낼 양 고정" (exact_in) vs "받을 양 고정" (exact_out). 지갑 UI 가 "최소 받음 / 최대 지불" 라벨을 분기할 때 사용 |
| `protocolId` | `uniswap.v2` / `uniswap.v3` / `uniswap.v4` / `pancakeswap.v2/v3/smartRouter/stableSwap/infinity` / `balancer.v2` / `unknown` | 어느 DEX 의 어느 generation 인가. 같은 selector 라도 Uniswap V2 와 Pancake V2 는 protocolId 로 분기 |

### 1.2 토큰 / 금액

| 필드 | 의미 | 사용자 관점 |
|---|---|---|
| `tokenIn` | 사용자가 **내보낼** 자산 (AssetRef) | UI 의 "From" 항목 |
| `tokenOut` | 사용자가 **받을** 자산 (AssetRef) | UI 의 "To" 항목 |
| `amountIn` | 입력 양 (AmountConstraint) | exact_in 이면 `kind=exact`, exact_out 이면 `kind=max` (슬리피지 상한) |
| `amountOut` | 출력 양 (AmountConstraint) | exact_in 이면 `kind=min` (슬리피지 하한), exact_out 이면 `kind=exact` |
| `valueWei` | 트랜잭션 envelope.value 의 복사본 (10진 문자열) | native (ETH) 입력 swap 시 nonzero. UI 의 "지불 ETH" 표시용 |

### 1.3 주체 / 시점

| 필드 | 의미 | 사용자 관점 |
|---|---|---|
| `sender` | swap 자금을 *내는* 주소 | 보통 사용자 본인 (`request.from`). Balancer relayer / UR sentinel 케이스에선 다를 수 있음 |
| `recipient` | swap 결과를 *받는* 주소 | UI 의 "받을 주소" — `from` 과 다르면 "다른 EOA 로 송금" 시그널 |
| `recipientEqualsActor` | derived bool: `recipient == request.from` | `false` 면 **제3자 송금 패턴**. phishing 1차 신호. UR sentinel (0x…0001/0002) 인 경우 decoder 는 `undefined`, aggregator 가 sentinel 해석 후 채움 |
| `deadline` | unix timestamp (DecimalString). 이 시각 지나면 revert | V2/V3 mandatory. V4/V4Router 일부는 없음 |
| `deadlineHorizonSeconds` | derived int: `deadline - block.timestamp` | "지금부터 N 초 안에 mining 되어야" — 비정상적으로 길거나 음수면 의심 |

### 1.4 수수료 정보 (V3/V4 한정)

| 필드 | 의미 |
|---|---|
| `fee.tier` | raw fee 값. V3=hundredths-of-bip (3000 = 0.30%), V4=PoolKey.fee (uint24, 0x800000 bit = dynamic 마커) |
| `fee.bps` | derived bps. V3 는 `tier/100`. V4 dynamic 풀이면 `undefined` (★ B6/G6 fix — dynamic 풀의 static bps 는 의미 없음) |
| `fee.dynamic` | V4 dynamic fee 마커. `(raw & 0x800000) !== 0` |

### 1.5 경로

| 필드 | 의미 | 사용자 관점 |
|---|---|---|
| `route.kind` | `single_hop` / `multi_hop` / `split_route` / `batch` / `opaque` | "직접 풀 1개" / "여러 풀 거침" / "여러 경로 병렬 분할" / "Balancer batchSwap" / "디코드 불가" |
| `route.hops[]` | 각 hop 의 상세 (SwapHop) | UI 의 "경로" 시각화: TOKEN_A → POOL_X → TOKEN_B → POOL_Y → TOKEN_C |
| `route.hops[i].id` | hop 식별자 (예: `h#0`) | 내부 참조용 |
| `route.hops[i].protocol` | hop 이 사용한 DEX (예: `uniswap.v3`) | mixed-protocol route 분석 |
| `route.hops[i].pool` | hop 의 pool 주소 | calldata 로 derive 가능할 때만 |
| `route.hops[i].feeTier` / `feeBps` | hop 의 수수료 | V3/V4 |
| `route.hops[i].percent` | split 시 이 hop 의 비중 (%) | split_route 한정 |
| `route.hops[i].confidence` | 이 hop 정보의 신뢰도 | `high` / `medium` / `low` / `unknown` / `unavailable` |
| `route.rawRoute` | raw bytes (V3 packed path 등) 보존 | 디버깅 / 재현 가능성 |
| `route.assets` | Balancer batchSwap 의 asset list (0x0 = native ETH) | **`_kind=batch_swap` 한정** (이전엔 description-only, 지금은 schema 강제) |
| `route.limits` | Balancer batchSwap 의 signed int 한도. 양수 = 사용자가 지불할 max input, 음수 = 사용자가 받을 min output | 동상 |

### 1.6 슬리피지

| 필드 | 의미 |
|---|---|
| `slippage.amountOutMin` | 최소 받음 (DecimalString) — `amountOut.kind=min` 일 때 동일 값 복사 |
| `slippage.amountInMax` | 최대 지불 — `amountIn.kind=max` 일 때 동일 값 복사 |
| `slippage.bps` | 사용자가 의도한 슬리피지 허용 비율 (bps). quote 와의 차이로 derive 가능할 때 |
| `slippage.source` | `calldata` / `quote` / `derived` / `unknown` — bps 가 어디서 왔는가 |

### 1.7 결제 방식

| 필드 | 의미 |
|---|---|
| `settlement.kind` | `direct_pool` (사용자 → pool 직접) / `router` (V2/V3 router 경유) / `aggregator` (1inch, UR 등) / `vault` (Balancer) / `intent` (UniswapX, CoW) / `unknown` |

### 1.8 V4 hook 정보 (V4 한정)

| 필드 | 의미 |
|---|---|
| `hookFlags` | V4 hook 주소의 마지막 14 bits decode 결과. `["beforeSwap", "afterSwap", "beforeSwapReturnsDelta", "afterSwapReturnsDelta"]` 부분집합 |

→ hook 이 swap 의 의미를 바꿀 수 있음. UI 는 hook 여부를 "warning: 이 swap 은 외부 hook 컨트랙트가 개입" 으로 표시 권장.

### 1.9 시세 (호스트가 oracle 로 채움)

| 필드 | 의미 |
|---|---|
| `inputUsd` | 입력 양 × tokenIn 시가 (UsdValuation) |
| `outputUsd` | 최소 받음 × tokenOut 시가. **사용자가 "최악의 경우 받을 USD 가치"** |
| `expectedOutputUsd` | 입력의 fair price 기반 **기대 출력 USD**. slippage 분석의 base |

`UsdValuation = { value: "1234.56", asOfTs, sources: ["chainlink", …], staleSec }`.

---

## 2. `liquidity.json` — LiquidityActionFields

`category = "liquidity"` 인 action 의 fields. **8 종 `_kind`** 를 단일 shape 로 통합:

- `add_liquidity` / `remove_liquidity` — V2 family
- `mint_position` / `increase_liquidity` / `decrease_liquidity` / `burn_position` — V3/V4 NPM family
- `join_pool` / `exit_pool` — Balancer family

### 2.1 식별

| 필드 | 의미 |
|---|---|
| `_kind` | 위 8 종 중 하나. `NormalizedAction.type` 와 1:1 |
| `operation` | `add` / `remove` / `join_pool` / `exit_pool` / `increase` / `decrease` / `mint_position` / `burn_position` — `_kind` 와 1:1 매핑 표 (docs/05 §5 부록) |
| `protocolId` | swap 의 그것과 동일 enum |

### 2.2 풀 / 토큰 / 금액 (모든 family 공통)

| 필드 | 의미 | 사용자 관점 |
|---|---|---|
| `pool` | 단일 pool 주소 (Address) | V2 pair / V3 pool / V4 PoolKey-derived pool address |
| `poolId` | bytes32 pool id (Hex) | Balancer poolId / V4 PoolId |
| `tokens[]` | 풀의 underlying 토큰 리스트 (AssetRef[]) | V2/V3/V4 는 길이 2, Balancer 는 N (최대 8). V3/V4 NPM 의 increase/decrease 는 빈 배열 (host:onchain 보강 필요 — 아래 ※) |
| `amounts[]` | tokens[i] 와 1:1 (AmountConstraint[]) | operation 별 의미:<br>• add: 예치 양 (`exact` 또는 `estimated`)<br>• remove: 인출 양 (`min` 보장 하한)<br>• join_pool: maxAmountsIn (`max`)<br>• exit_pool: minAmountsOut (`min`) |

※ V3/V4 NPM 의 `increaseLiquidity` / `decreaseLiquidity` 는 calldata 에 `tokenId` 만 있고 토큰 주소가 없음. decoder 는 빈 `tokens=[]` 로 두고, aggregator 의 `enrichLiquidityWithPosition` 헬퍼가 host:onchain (`NPM.positions(tokenId)`) 로 보강 (★ #39 fix).

### 2.3 V3/V4 NPM 전용 (concentrated liquidity)

| 필드 | 의미 |
|---|---|
| `positionTokenId` | ERC721 position NFT id (DecimalString) |
| `feeTier` | swap fee tier — **단, max_fee_bps 분석에 포함되지 않음** (LP fee 는 swap 시 발생, 본 action 의 fee 와 의미 다름 — ★ 세션 9 #41) |
| `tickLower` | position 의 가격 하한 tick |
| `tickUpper` | position 의 가격 상한 tick |
| `liquidityDelta` | signed int (IntDecimalString). V4 `modifyLiquidity` 의 raw delta — **음수 = remove**. V3 NPM 의 양수 uint128 liquidity 도 본 필드 |

### 2.4 Balancer 전용

| 필드 | 의미 |
|---|---|
| `userData` | Balancer joinPool/exitPool 의 raw `userData` bytes (Hex). 그 안의 첫 32 bytes 가 JoinKind/ExitKind enum — single-asset-join 등 의미 분기 |

### 2.5 주체 / 시점

| 필드 | 의미 |
|---|---|
| `sender` | 자금/LP 토큰을 *내는* 주소 |
| `recipient` | LP 토큰 (V2) / position NFT (V3/V4) / underlying 토큰 (remove/exit) 받을 주소 |
| `recipientEqualsActor` | derived bool. swap 과 동일 의미 |
| `deadline` | unix timestamp |
| `deadlineHorizonSeconds` | derived int |
| `valueWei` | envelope.value 복사본. V3/V4 NPM mint/increaseLiquidity 같은 **payable 함수에서 ETH 입력 측 input** (★ ADR-007 SD-4) |

### 2.6 슬리피지

| 필드 | 의미 |
|---|---|
| `slippage.minLiquidity` | add 의 `min_mint_amount` — **최소 받을 LP 토큰 양** |
| `slippage.maxAmountIn[]` | remove_liquidity_imbalance 등의 `max_burn_amount` 또는 N-token max-in 한도 — tokens[] 와 1:1 |
| `slippage.minAmountOut[]` | remove 의 `min_amounts[N]` — tokens[i] 별 최소 받음 |
| `slippage.source` | 동일 enum |

### 2.7 결제 / 시세

| 필드 | 의미 |
|---|---|
| `settlement.kind` | `router` / `manager` (V3/V4 NPM) / `vault` (Balancer) / `direct_pool` / `unknown` |
| `totalInputUsd` | Σ(amounts[i] × tokens[i] 시가) USD — UI 의 "총 예치 가치" |

---

## 3. 메타 annotation — 각 필드 어디서 왔는가

본 schema 의 모든 필드에는 `x-source` 라벨이 붙어 있어 "누가 이 값을 채우는가" 가 명시됩니다.

| label | 의미 | 사용자 관점 |
|---|---|---|
| `action-derived` | calldata 에서 디코더가 직접 채움 | 정보가 트랜잭션 자체에 포함 — 항상 표시 가능 |
| `adapter:metadata` | 라우터/풀 구조 정보 (route hops, slippage, settlement, hookFlags) | 디코더의 구조 분석 결과 — 대부분 신뢰 가능 |
| `host:oracle` | 지갑/서버의 가격 oracle 이 채움 (inputUsd, outputUsd, expectedOutputUsd) | 별도 oracle 신뢰 — staleSec 확인 필요 |
| `host:onchain` | NFT position / pool config 등 on-chain 조회로 보강 | V3/V4 NPM increase/decrease 의 tokens[] 등 |

지갑 UI 는 host:oracle / host:onchain 결과가 없을 때 "시세 정보 가져오는 중…" 로 graceful degradation 가능합니다.

---

## 4. 자주 묻는 질문

**Q. `_kind` 가 `swap` 인데 `route.assets[]` / `route.limits[]` 가 있을 수 없다고요?**
A. 네. 두 필드는 Balancer V2 batchSwap (`_kind=batch_swap`) 전용입니다. schema 의 `if/then` 으로 강제됩니다. swap 인스턴스에 잘못 들어가 있으면 validator 가 reject.

**Q. `kind="erc20"` 인데 `address` 가 없는 AssetRef 는요?**
A. 거부됩니다. erc20/erc721/erc1155 는 address 필수 (schema 의 if/then). native/unknown 만 address 생략 허용.

**Q. `amountIn.kind` 와 `mode` 의 관계가 일관되지 않으면?**
A. mode=exact_in 이면 amountIn.kind ∈ {exact, max, unlimited}, amountOut.kind ∈ {min, estimated, unknown} 이 권장. 다만 schema 가 invariant 로 강제하지는 않음 — 의미 정합성은 호출자/분석 레이어 책임.

**Q. NPM increaseLiquidity 인스턴스의 `tokens[]` 가 비어 있는데 정상인가요?**
A. 정상입니다. calldata 에 tokenId 만 있고 토큰 주소가 없기 때문. aggregator 가 host:onchain 으로 `NPM.positions(tokenId)` 조회 후 채웁니다. host lookup 미주입 시 빈 배열 그대로 보존 — 그 경우 confidence 강등.

**Q. V4 swap 에 `recipient` 가 항상 ctx.actor 인 이유?**
A. V4 의 lock/unlock 패턴 — swap action 자체에는 recipient 가 없고, 별도 TAKE / TAKE_PORTION opcode 가 최종 수령자 결정. decoder 가 형식 일관성 위해 `recipient=ctx.actor` 채움 — 본 케이스에서 `recipientEqualsActor` 는 항상 `true`, 분석적 가치 없음.

---

## 5. 참고

- 원본 schema: `action-fields/swap.json`, `action-fields/liquidity.json`
- 공통 primitive: `_common.json`
- 함수 인벤토리: `docs/10-function-inventory.md`
- ADR 이력: `docs/adr/`
