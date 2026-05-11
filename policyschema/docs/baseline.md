# `policyschema` baseline

이 문서는 `policyschema` v0.1의 **데이터 모델 헌법**입니다. 정책 엔진(Cedar) 직전 단계에서 모든 트랜잭션·서명을 정규화한 결과(`NormalizedRequestV2`)의 의미를 정의합니다.

---

## 0. 계층 다이어그램

```
┌──────────────────────────────────────────────────────────┐
│             정책 평가 (Cedar 등) — 외부                  │
└──────────────────────────────────────────────────────────┘
                          ▲ NormalizedRequestV2
┌──────────────────────────────────────────────────────────┐
│   스키마 정의 (이 크레이트):                              │
│   ActionType / Category / Fields / Extension              │
│   + dispatch table 정의 (메타데이터)                       │
└──────────────────────────────────────────────────────────┘
                          ▲ ActionFields
┌──────────────────────────────────────────────────────────┐
│   세미-어댑터 (이 크레이트):                              │
│   args JSON + Context → 정규화된 ActionFields             │
│   - build_*_fields 함수, classify_call/slipstream/v4_swap │
└──────────────────────────────────────────────────────────┘
                          ▲ args (JSON)
┌──────────────────────────────────────────────────────────┐
│   풀 어댑터 — raw calldata → ABI 디코드 (alloy-sol-types) │
│   (외부, 정책 엔진 어댑터 팀)                             │
└──────────────────────────────────────────────────────────┘
                          ▲
                  지갑 / dApp의 eth_sendTransaction · eth_signTypedData
```

policyschema는 **스키마 정의 + 세미-어댑터 두 계층**을 책임집니다. *풀 어댑터*(raw calldata 디코드)와 *정책 평가*는 외부.

---

## 1. Scope / Out-of-scope

### 다루는 것
- `eth_sendTransaction` 형태의 EVM 트랜잭션 정적 해석
- `eth_signTypedData_v4` 형태의 EIP-712 서명 흐름
- DEX swap (Uniswap V2/V3/V4/UR, PancakeSwap V2/V3/SmartRouter/UR/Infinity, Aerodrome V1/Slipstream)
- 렌딩의 이자 핵심 4종 (supply/withdraw/borrow/repay) — Aave V3, Morpho Blue
- LST (Lido stETH·wstETH·WithdrawalQueue)
- EIP-712 서명: Permit2 (6 primaryType), EIP-2612, EIP-712 Other (catch-all)
- Universal Router `execute(...)` 메타 라우터 + 자식 atomic action 분해

### 다루지 않는 것 (out-of-scope)
- ❌ Liquidity / Position 액션 (add/remove/mint/burn/increase/decrease/collect)
- ❌ Hyperliquid (v0.1 보류 — spot DEX 위치 확정 후 v0.2)
- ❌ Perp 포지션 변경
- ❌ RWA / Restaking / Bridge
- ❌ 렌딩 부가 함수 (collateral toggle, flash loan, liquidation)
- ❌ Rocket Pool / Mantle (LST는 Lido만)
- ❌ Spark Lend (렌딩은 Aave V3 + Morpho Blue만)
- ❌ Host enrichment (Oracle USD, Portfolio fraction, Approvals coverage, StatWindows) — 정책 엔진 측에서 후속 단계로 enrich
- ❌ Cedar 정책 평가 자체
- ❌ Effects / 시뮬레이션 / EIP-1559 estimateGas

---

## 2. Core data model

`NormalizedRequestV2`는 7+1축 구조:

```
schemaVersion + Request → Targets → DecodedCalls → Actions → Extensions → Confidence → Raw
                                                                ↑
                                                            + Category
```

| 축 | 역할 | 모듈 |
|---|---|---|
| `request` | 원본 트랜잭션 또는 typed-data | `request.rs` |
| `targets` | 호출이 거치는 모든 컨트랙트 (router, pool, token, hook, …) | `target.rs` |
| `decodedCalls` | ABI 디코드된 호출 (외부/내부/UR command/multicall item …) | `call.rs` |
| `actions` | 사용자 단위 의미 행위 — **정책 평가의 1차 단위** | `action/` |
| `extensions` | 프로토콜 특수 데이터 (`namespace, data` 페어) | `extension.rs` |
| `confidence` | 단계별 신뢰도 (`unavailable | low | medium | high`) | `confidence.rs` |
| `raw` | 원본 wire 데이터 (audit·재현용) | `raw.rs` |

`Category`는 `Action`마다 부착되는 cross-cutting 분류 (§3 참조).

### Call ↔ Action 관계

**N:M**. 한 호출이 여러 action으로 분기하거나(예: UR `execute` → wrap + swap + sweep), 여러 call이 하나의 action으로 묶이기도 합니다(예: Balancer batchSwap step들 → 단일 batch swap action). `Action.derived_from_call_ids: Vec<String>`이 이 관계를 보존합니다.

---

## 3. ActionCategory + ActionType

### ActionCategory (13종)
정책 작성에서 cross-cutting 룰의 단위. *"모든 swap에 대해 X"*, *"모든 sign에 대해 Y"* 같은 표현이 자연스러워집니다. v0.1 보강에서 6종 → 13종 확장 (Bridge·Perp는 의도적 제외).

| Category | 설명 |
|---|---|
| `swap` | 토큰 교환 (`Swap`, `BatchSwap`, `HookedOperation`, `Wrap`, `Unwrap`) |
| `liquidity` | 풀 LP 지분 (`AddLiquidity`, `RemoveLiquidity`, `JoinPool`, `ExitPool`, V3/V4 NPM 5종) |
| `lending` | 렌딩 11종 (`Supply`, `Borrow`, `Repay`, `WithdrawCollateral`, ..., `MintUnbacked`) |
| `liquid_staking` | LST (`Stake`, `UnstakeRequest`, `ClaimUnstake`, `WrapReceipt`, `UnwrapReceipt`) |
| `restaking` | EigenLayer/etherfi/Renzo 등 8종 |
| `rwa` | Centrifuge/Ondo/Securitize/BlackRock 8종 |
| `governance` ⭐ | Compound/Aave/UNI/OZ Governor (`GovernancePropose/Vote/Execute/Delegate`) |
| `nft` ⭐ | Seaport/Blur/X2Y2 (`NftMint/Transfer/Buy/Sell`) |
| `vault` ⭐ | ERC-4626/Yearn (`VaultDeposit/Withdraw`) |
| `utility` | 가로지르는 보조 (approval, permit, transfer, claim_rewards, multicall, sign_message, airdrop_claim, merkle_claim, wrap, unwrap) |
| `aggregation` | UR `execute(...)` 컨테이너 (`RouterPlan`, `promote=false`) |
| `sign` | EIP-712 서명 6종 (`SignPermit2*`, `SignEip2612Permit`, `SignEip712Other`, `SignSafeTx`, `SignSessionKey`) |
| `unknown` | 미분류 catch-all |

### ActionType (72종) → Category 매핑 요약

| Category | ActionType count | 비고 |
|---|---|---|
| Swap | 5 | swap, batch_swap, hooked_operation, wrap, unwrap |
| Liquidity | 9 | v1 4 + V3/V4 NPM 5 |
| Lending | 11 | v1 7 + Aave 추가 4 |
| LiquidStaking | 5 | Lido 기준 |
| Restaking | 8 | EigenLayer 기준 |
| Rwa | 8 | Centrifuge 기준 |
| Governance | 4 | propose/vote/execute/delegate |
| Nft | 4 | mint/transfer/buy/sell |
| Vault | 2 | deposit/withdraw |
| Utility | 10 | approval/permit/transfer 등 |
| Aggregation | 1 | router_plan |
| Sign | 6 | Permit2/EIP-2612/Other/SafeTx/SessionKey |
| Unknown | 1 | catch-all |
| **합계** | **72** | |

전체 매핑은 `src/action/kind.rs`의 `ActionType::category()` 메서드 참조.

### Wrap / Unwrap을 Swap에 흡수한 이유

stETH ↔ wstETH·ETH ↔ WETH 같은 1:1 토큰 변환은 의미상 *교환*입니다. swap 카테고리에 두면 "모든 swap에 대해 X" 정책이 자연스럽게 wrap에도 적용됩니다. ActionType은 별도(Wrap/Unwrap)로 남겨, 정책 측이 필요하면 구분할 수 있게 했습니다. `ActionFields`는 `SwapFields`를 그대로 재사용 (별도 `WrapFields` 만들지 않음).

---

## 4. 공통 fragment (`$defs`)

`src/types.rs`에 정의된 공유 타입 — 여러 ActionFields가 embed해서 재사용:

| Fragment | 사용처 | 역할 |
|---|---|---|
| `Token` | 모든 ActionFields | chainId × address × symbol × decimals × isNative |
| `AmountSpec` | 모든 ActionFields | uint256 raw + 의미 분류 |
| `AmountKind` | `AmountSpec` 안 | `Exact` / `Min` / `Max` / `Unlimited` / `Unspecified` |
| `PoolKey` | `uniswap.v4`, `aerodrome.slipstream` | V4-style 풀 식별자 |
| `RecipientFields` | `SwapFields`, `LendingFields`, `StakingFields` | recipient + `recipient_equals_actor` + `has_external_recipient` |
| `DeadlineFields` | `SwapFields`, `LendingFields`(향후), `StakingFields`(향후), `SignFields`, `AggregationFields` | deadline + `deadline_horizon_seconds` |

### 직렬화 규약

- **Address**: alloy mixed-case checksum (`0xA0b86991…`)
- **uint256**: 십진 문자열 (`"1000000000"`)
- **enum**: snake_case 문자열 (예: `"exact_in"`, `"swap"`)
- **AmountKind**: PascalCase 문자열 (`"Exact"`, `"Min"`, `"Max"`, `"Unlimited"`, `"Unspecified"`)
- **field name**: camelCase (예: `recipientEqualsActor`, `protocolIds`)

### `recipient_equals_actor` vs `has_external_recipient`

수학적으로 부정 관계지만 둘 다 둡니다. 정책에서 자연스러운 표현이 다르기 때문(`if recipient_equals_actor then …` vs `if has_external_recipient then …`). 정규화 단계에서 한 번만 계산해 두 boolean을 모두 채웁니다.

---

## 5. Extension namespace 정책

총 **41종** namespace (v0.1 보강에서 13종 → 41종 확장; Balancer V3 추가). 명명 규칙은 `<protocol>.<component>` 또는 단일 `<protocol>` (component가 단일이거나 통합된 경우).

| namespace | 카테고리 | 역할 |
|---|---|---|
| `uniswap.v2` | DEX | Uniswap V2 Router02 (constant-product AMM, transparent path) |
| `uniswap.v3` | DEX | Uniswap V3 SwapRouter / SwapRouter02 (concentrated liquidity) |
| `uniswap.v4` | DEX | Uniswap V4 PoolManager (singleton + Hook) |
| `uniswap.universalRouter` | DEX (Aggregation) | Uniswap UR `execute(commands, inputs)` |
| `pancakeswap` | DEX | **통합** — `data.component` ∈ {`v2`, `v3`, `smartRouter`, `universalRouter`, `infinity`} |
| `aerodrome.v1` | DEX | Base 체인 Solidly fork |
| `aerodrome.slipstream` | DEX | Aerodrome의 Uniswap V3 fork (tickSpacing 기반) |
| `aave.v3` | Lending | Aave V3 Pool (supply/withdraw/borrow/repay) |
| `morpho.blue` | Lending | Morpho Blue (marketParams 기반) |
| `lido` | LST | **통합** — `data.component` ∈ {`stETH`, `wstETH`, `withdrawalQueue`} |
| `permit2` | Sign | Uniswap Permit2 canonical contract |
| `eip2612` | Sign | ERC20 표준 EIP-712 permit |
| `eip712` | Sign | catch-all EIP-712 |
| `erc20` | Token | 일반 ERC20 작업 |
| `weth` | Token | WETH wrap/unwrap |

### 통합 namespace의 `data` 구조

`pancakeswap` / `lido`는 단일 namespace로 통합하되, `data.component` 필드로 sub-component를 식별합니다:

```jsonc
// "lido" 예시
{
  "namespace": "lido",
  "data": {
    "component": "stETH" | "wstETH" | "withdrawalQueue",
    // component별 추가 필드:
    //   stETH:           { referral }
    //   wstETH:          {}
    //   withdrawalQueue: { amounts[], requestIds[], requestId }
  }
}

// "pancakeswap" 예시
{
  "namespace": "pancakeswap",
  "data": {
    "component": "v2" | "v3" | "smartRouter" | "universalRouter" | "infinity",
    // component별 추가 필드 — extensions/pancakeswap.md 참조
  }
}
```

---

## 6. SwapFields 설계 + x-adapter-mapping

`SwapFields`의 모든 필드와 그 출처는 `docs/protocol-comparison.md` §1에 표 형태로 정리. 핵심 원칙:

- **공통 필드만 surface**: 모든 swap-class 프로토콜에 의미가 동등한 필드만 코어에 둠 (token_in/out, amount_in/out, mode, route, recipients, deadlines, max_fee_bps, has_zero_min_output)
- **프로토콜 특수 필드는 Extension**: V3 `sqrtPriceLimitX96`, V4 `hooks`/`hookData`, Aerodrome V1 `stable[]` 등은 모두 `extensions[]`에
- **`x-source` 메타**: 각 필드 doc comment에 `action-derived` (calldata 직접 디코드) / `adapter:metadata` (추론·정적 매핑) / `derived` (다른 필드에서 계산) 명시
- **`x-adapter-mapping`**: 각 필드가 각 프로토콜의 calldata 어디서 오는지 doc comment에 표 형태로 (예: V2는 `path[0]`, V3는 `params.tokenIn`, V4는 `PoolKey.currency0/1`)

### 모드·라우트 의미론

| `mode` | 의미 | `amount_in.kind` | `amount_out.kind` |
|---|---|---|---|
| `ExactIn` | 입력 정확, 출력 최소 보장 | `Exact` | `Min` |
| `ExactOut` | 출력 정확, 입력 최대 한도 | `Max` | `Exact` |

`route` 변형 4종:
- `SingleHop`: 단일 풀
- `MultiHop`: 직선 경로 (V2 path, V3 encoded path)
- `Split`: 병렬 분기 (PancakeSwap SmartRouter, UR multi-command)
- `Batch`: Balancer batchSwap 형태 (v0.1 범위 외, 모양만 정의)

### DexFacts 11필드 반영 상태

사용자가 제시한 `DexFacts` 11필드 중:
- ✅ **6필드 차용** (정적 디코드 가능): `protocol_ids`, `input_tokens`, `output_tokens`, `max_fee_bps`, `has_zero_min_output`, `has_external_recipient` (RecipientFields 안)
- ❌ **5필드 제외** (모두 host enrichment): `total_input_usd`, `total_min_output_usd`, `total_input_fraction_of_portfolio_bps`, `allowances_cover_inputs`, `window_stats`

---

## 7. LendingFields / StakingFields / SignFields 설계

각 ActionFields의 필드별 출처는 `docs/protocol-comparison.md` §2/§3/§4 참조.

### LendingFields 핵심 결정
- **이자 핵심 4종만**: supply/withdraw/borrow/repay. flash loan·liquidation·collateral toggle은 v0.1 외.
- **Aave V3 vs Morpho Blue 차이**:
  - Aave: `Pool` 컨트랙트의 4개 함수, `interestRateMode` enum 존재
  - Morpho: `marketParams` 5튜플로 시장 식별, `assets` 또는 `shares` 둘 중 하나만 nonzero
- **Repay-max 패턴**: `type(uint256).max`로 풀 잔여를 모두 갚는 케이스 — `AmountKind::Unlimited`로 표현

### StakingFields 핵심 결정
- **Lido만 v0.1 범위**: submit / requestWithdrawals / claimWithdrawal
- **Wrap/Unwrap 분리**: stETH ↔ wstETH wrap/unwrap은 SwapFields 사용 (1:1 token conversion)
- **NFT 발급**: requestWithdrawals는 NFT(unstETH)를 발급하지만 `withdrawal_request_id`는 calldata에 없음 (event-derived). v0.1에서는 `None`.

### SignFields 핵심 결정
- **`signer` = `actor`**: 같은 의미. SignFields에선 `signer`라는 별칭으로 surface.
- **`SignSemantic` discriminated union**: primaryType별 풀 디코드 (Permit2Approve / Permit2TransferFrom / Eip2612Permit / Other)
- **Catch-all 보존**: 인식되지 않은 EIP-712는 `Other { types_json, message_json }`로 도메인·메시지를 *원본 그대로* 보관

---

## 8. RouterPlan / N:M Call ↔ Action 매핑 정책

Universal Router `execute(commands, inputs)`은 **부모 RouterPlan + 자식 Action들**로 분해됩니다.

```
execute(bytes commands, bytes[] inputs)
│
└─ 부모 Action a#0
   { category: Aggregation, type: RouterPlan, fields: AggregationFields { commands_hex, mask, child_count, … } }
   │
   ├─ 자식 a#1: { category: Swap, type: Swap, fields: SwapFields, parent_action_id: "a#0" }
   ├─ 자식 a#2: { category: Sign, type: SignPermit2Approve, parent_action_id: "a#0" }
   └─ 자식 a#3: { category: Swap, type: Swap, parent_action_id: "a#0" }
```

### opcode 마스킹 규칙

| Family | Mask | 비트 7 (high) 의미 |
|---|---|---|
| Uniswap UR | `& 0x7f` | `0x80` = `FLAG_ALLOW_REVERT` |
| PancakeSwap UR | `& 0x3f` | (다른 opcode 공간) |
| Uniswap V4 Actions | (마스킹 없음) | — |

**디코딩 첫 단계는 family 판별**입니다. `tx.to` 주소로 결정.

### `promote` 정책

- 부모 RouterPlan은 `promote = false` (정책 평가 단위가 아니라 *컨테이너*)
- 자식 swap·sign은 `promote = true` (정책 평가 단위)
- WRAP_ETH / UNWRAP_WETH / SWEEP / PAY_PORTION 같은 정산 보조 opcode는 자식 Action으로 promote 하지 않고 부모의 Extension에만 기록

---

## 9. Confidence 정책

`Confidence` 4단계: `Unavailable` < `Low` < `Medium` < `High`.

각 단계마다 `ConfidenceReport.stages[Stage]`에 별도 기록:
- `Request` — 원본 wire 형식이 정상인지
- `TargetIdentification` — 모든 target 주소를 식별했는지
- `AbiDecode` — 모든 함수가 검증된 ABI로 디코드됐는지
- `ProtocolDecode` — 프로토콜 특정 의미를 정확히 파싱했는지
- `SemanticAction` — 사용자 단위 action으로 정확히 매핑됐는지
- `RouteDecode` — 라우트(path/encoded path/PoolKey)가 정확히 분해됐는지
- `AmountInterpretation` — amount/slippage 의미가 정확한지

`overall`은 단계별 최저값 (보수적). 추가 메모는 `notes: Vec<String>`.

### 카테고리별 ceiling
- Fee-on-transfer 변형: `medium` ceiling (router가 도달 입력을 정확히 알 수 없음)
- 폐쇄소스 프로토콜: `medium` ceiling
- 인식되지 않은 EIP-712 (`SignEip712Other`): `low` ceiling

---

## 10. fixture walkthrough

대표 fixture로 `01-uniswap-v2-swap`을 단계별 추적:

1. **`fixtures/01.input.json`**: `request`(eth_sendTransaction) + `targets`(router, USDC, WETH) + `decodedCalls` 1개(swapExactTokensForTokens) + `raw`. ActionFields는 *없음*.
2. 정규화 파이프라인 (구현 단계별):
   - **Decode**: `0x38ed1739` 셀렉터 + ABI로 인자 추출
   - **Classify**: dispatch 룩업 → `(category=Swap, type=Swap)`
   - **Field decode**: `SwapFields { mode: ExactIn, input_tokens: [USDC], output_tokens: [WETH], amount_in: Exact, amount_out: Min, route: SingleHop, max_fee_bps: Some(30), recipients: { recipient_equals_actor: true }, … }`
   - **Extension**: `uniswap.v2 { path: [USDC, WETH], supporting_fee_on_transfer: false }`
   - **Confidence**: 모든 단계 `high`
3. **`fixtures/01.expected.json`**: 위가 모두 채워진 `NormalizedRequestV2`.

각 fixture pair는 *입력→출력* 변환의 *증명*입니다. 다른 fixture(02~21)도 동일 패턴을 따릅니다.

---

## 11. Versioning strategy

- **schemaVersion**: `"policyschema-v0.1.0"` (현재).
- **호환성 정책**:
  - **Patch (`0.1.0 → 0.1.1`)**: 버그 수정. 와이어 호환성 유지.
  - **Minor (`0.1.0 → 0.2.0`)**: 새 ActionType / 새 Extension namespace 추가. 기존 enum variant 명/직렬화 형식은 안 변경 → 와이어 호환.
  - **Major (`0.1.0 → 1.0.0`)**: 호환성 깨짐. ActionFields 구조 변경 등.
- **새 ActionType 추가 시 BC**: enum에 variant 추가 + 기존 variant들의 직렬화 명·필드 유지 → 구버전 정책이 *기존 ActionType*에는 그대로 동작.
- **새 Extension namespace 추가**: enum variant 추가만. 정책이 알지 못하는 namespace는 무시 (필드 접근 시 `None`)
- **deprecated 정책**: 한 minor 버전 동안 deprecation 노트, 다음 major에서 제거.

향후 v0.2 후보:
- Hyperliquid (spot DEX 위치 확정 후)
- Liquidity action (add/remove/mint/burn/increase/decrease/collect)
- 추가 LST (Rocket Pool, Mantle mETH)
- Spark Lend
