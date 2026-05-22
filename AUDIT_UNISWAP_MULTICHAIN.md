# AUDIT — Uniswap Tier A/B Multi-Chain 정합화 (Phase 7B / Phase 5 audit)

> **Scope**: ScopeBall Uniswap entry-point router registry manifest 의 13 target chain
> `[1,8453,10,42161,137,43114,81457,56,42220,57073,130,480,7777777]` multi-chain 정합화.
> per-address split (324 manifest) + UR manifest 갱신 + SR02 multicall overload 2 + permit2/permit
> emit 수정 + Tier B World Chain UR 수정.
> **Method**: QuillAudits BSA — Behavioral Decomposition → Threat Modeling → Adversarial
> Simulation → Risk Scoring. **read-only.**
> **1차 출처**: `Uniswap/contracts` repo `deployments/<chainId>.md` (13 chain 전부 fetch),
> on-chain `cast code` / `cast call` (Optimism / Base / BNB / Polygon / Arbitrum / Unichain /
> World Chain / Avalanche / Blast / Celo / Zora mainnet RPC).
> **HEAD**: worktree `phase7b-uniswap` / branch `worktree-phase7b-uniswap`.

---

## 0. 요약 (Risk Scoring)

| severity | count | findings |
|---|---|---|
| **P0** | 0 | — |
| **P1** | 1 | F-1 Optimism UR 주소 transcription 오류 (`0xde20…Cd9AF`) |
| **P2** | 2 | F-2 UR cross-product spurious callkey (754 callkey, 정량) · F-3 declarative UR path 에 `is_uniswap_universal_router` 게이트 부재 |
| **P3** | 3 | F-4 seed bundle chain 축소의 side-effect (Base/OP/ARB V2 swap = JIT 의존) · F-5 UR manifest `to[]` 에 v1.2 과거 주소 혼입 (의도적이나 문서화 부재) · F-6 `0x7a250d56` World Chain 잔존 (V2Router02 는 실재하나 UR 아님) |
| **Info** | 2 | I-1 selector 6/6 정확 · I-2 permit2/permit positional path 수정 정합 |

**핵심 판정**: 80 주소 matrix 중 **79 개 정확**, **1 개 오류** (Optimism UR). per-address split·
selector·permit2 수정·Tier B World Chain 수정은 모두 **정합**. 잔존 P2 는 보안 도구로서
"틀린 verdict" 가 아니라 "주소 식별 정밀도 부족" (spurious callkey) 범주 — 단, F-1 은
실제 Optimism UR tx 가 declarative path 를 **영구 miss** 하게 만들어 P1.

---

## 1. Behavioral Decomposition — audit 대상 동작 모델

ScopeBall 의 declarative adapter 평가 경로 (서명 직전 분석):

```
tx (chain_id, to, calldata)
  → extractSelector → CallMatchKey{chain_id, to, selector}
  → resolveAdapter: Layer1 mounted → negative cache → Layer2 cache
       → Layer3 JIT: registry GET index/by-callkey/<chain>__<to>__<sel>.json
  → WASM declarative_route_request_json:
       bridge lookup (chain_id, to_lower, sel_lower) → decoder_id
       → decode_with_json_abi(bundle.abi_fragment.abi, calldata)
       → DeclarativeMapper DSL → ActionEnvelope[]
  → Cedar 평가 → Verdict
```

핵심: **주소 식별의 단일 게이트 = callkey 존재 여부**. registry `index/by-callkey/` 와
WASM `register_bridge_entries` 는 **둘 다 bundle `match.{chain_ids × to × selector}`
cross-product** 로 생성된다 (`build-index.ts:190`, `declarative_exports.rs:99-113`).
→ matrix 주소가 틀리면 callkey 가 틀리고, declarative path 가 통째로 miss/오인.

value flow: 이 component 는 calldata 를 **읽기만** 함 (정적분석). 자금 이동 없음.
따라서 "fund loss" 류 P0 는 구조적으로 발생 불가 — 영향은 **verdict 정확성**
(miss = 사용자가 분석 없이 서명 / mis-decode = 잘못된 intent 표시) 에 한정.

trust 가정:
- registry (Cloud Run proxy + private GCS) — bundle 무결성은 RFC8785 JCS + SHA-256.
- Tier B `UNISWAP_UR_TABLE` — UR opcode 의 단일 진실.
- `uniswap-deployments.json` matrix — **본 audit 의 1차 검증 대상**.

---

## 2. Threat Modeling — 공격 표면

| 표면 | 위협 | 본 audit 관련 finding |
|---|---|---|
| matrix 주소 부정확 | 틀린 주소 → callkey 오생성 → 실 tx miss / 무관 주소 hijack | **F-1** (Optimism UR 오류) |
| cross-product (UR 예외) | 13×29 조합 → 미배포 (chain,addr) callkey 양산 | **F-2** |
| declarative UR dispatch | Tier B allowlist 우회 → 임의 (chain,addr) 에 UR opcode table 적용 | **F-3** |
| per-address split 누락 | split 후 일부 chain 미커버 | F-4 (seed bundle), 그 외 정합 |
| selector 충돌 | overload selector 오기 → 잘못된 ABI 로 decode | I-1 (6/6 정확) |
| nested tuple path | eval.rs nested-named 미지원 → permit decode 실패 | I-2 (positional 수정 정합) |

---

## 3. Adversarial Simulation — finding 별 상세

### F-1 [P1] Optimism Universal Router 주소 transcription 오류

**Impact** — Optimism (chain 10) 에서 실제 사용 중인 Universal Router
(`UniversalRouterV2`) tx 가 declarative path 를 **영구 miss**. 사용자가 Optimism 에서
Uniswap UR swap 을 서명할 때 ScopeBall 이 intent 분해 없이 static fallback 으로만
처리 → swap 의 input/output token·amount·recipient 가 정책 평가에 안 들어감.

**Root cause** — `registry/scripts/uniswap-deployments.json` `universal-router["10"]` 3번째 원소:

```
"10": [
  "0xCb1355ff08Ab38bBCE60111F1bb2B784bE25D7e8",   // V1.2 — OK (on-chain 35919 byte)
  "0x851116D9223fabED8E56C0E6b8Ad0c31d98B3507",   // V2   — OK (on-chain 39001 byte)
  "0xde20EEE5398D3790a4D356e8925bB0DF7E6Cd9AF"    // ← 오류
]
```

1차 출처 `deployments/10.md` (Summary + Deployment History + heading 3곳 일치) 의
Optimism UniversalRouter = **`0xde20eee5398d3790a4d356e8925bd21ea65d99af`**.
matrix 값과 비교하면 주소 하위 절반이 다름:

```
matrix : 0xde20eee5398d3790a4d356e8925b  b0df7e6cd9af
md     : 0xde20eee5398d3790a4d356e8925b  d21ea65d99af
                                    ^^^^^^^^^^^^^^^^^^ 불일치 (앞 13 byte 만 우연 일치)
```

**on-chain 검증** (`cast code`, Optimism RPC `mainnet.optimism.io`):

| 주소 | Optimism | mainnet | Base |
|---|---|---|---|
| matrix `0xde20…Cd9AF` | **`0x` (코드 없음)** | `0x` | `0x` |
| md `0xde20…d99af` | **43479 byte (UR)** | — | — |

→ matrix 주소는 **어느 체인에도 배포되지 않은 주소**. 순수 오타 (cross-chain 혼동도 아님).

**전파 범위** — `gen-uniswap-ur.ts` 가 이 값을 UR manifest 의 `to[]` (cross-product
예외) 에 그대로 주입:
- `registry/manifests/uniswap/universal-router/execute@1.0.0.json` 27행
- `registry/manifests/uniswap/universal-router/execute-no-deadline@1.0.0.json` 27행
- `build-index.ts` 가 13 chain × 2 selector = **26 callkey** 생성
  (`index/by-callkey/{1,10,…}__0xde20eee5398d3790a4d356e8925bb0df7e6cd9af__0x{3593564c,24856bc3}.json`).
- **올바른 Optimism UR `0xde20…d99af` 의 callkey 는 index 에 0개** (`ls | grep` 확인).

**PoC sketch** (Foundry / 개념)
```
// Optimism 에서 UR swap 시도 — 실 배포 주소 0xde20…d99af 로 전송
vm.createSelectFork("optimism");
bytes memory calldata_ = abi.encodeWithSelector(0x3593564c, commands, inputs, deadline);
// ScopeBall: extractSelector → CallMatchKey{10, 0xde20…d99af, 0x3593564c}
// registry index 에 10__0xde20eee…d99af__0x3593564c.json 없음 → callkey MISS
// → declarative-route outcome="miss" → static fallback → UR opcode 분해 누락
assert(registryHas("10__0xde20eee5398d3790a4d356e8925bd21ea65d99af__0x3593564c") == false); // 영구 miss
```

**Remediation** — `uniswap-deployments.json` `universal-router["10"][2]` 를
`0xde20EEE5398D3790a4D356e8925bd21ea65d99af` (EIP-55) 로 교체 후
`gen-uniswap-ur.ts` 재실행 + `registry/ npm run build` → index 재생성 → 26 callkey 정정.
**검증 권장**: generator 에 "matrix 의 모든 주소에 `eth_getCode != 0x`" 정합성 체크
추가 (per-chain RPC) 했다면 본 오류는 build 시 잡혔음.

> **✅ 적용 완료 (worktree-phase7b-uniswap)** — matrix `universal-router[10][2]` →
> `0xde20EEE5398D3790a4D356e8925bD21Ea65D99Af` (`cast --to-checksum-address`) 교체 →
> `gen-uniswap-ur.ts` 재실행 → `npm run build` 재생성. callkey
> `10__0xde20…d99af__0x3593564c` 존재 + typo 주소(`…b0df7e6cd9af`) callkey 0개 확인.

---

### F-2 [P2] UR cross-product spurious callkey — 754 callkey 중 다수가 미배포 (chain,addr)

**Impact** — UR manifest 는 per-address split 의 의도적 예외로 cross-product 유지
(`gen-uniswap-manifests.ts` 주석: per_opcode_emit ~250줄 중복 회피). 결과:
`13 chain × 29 distinct addr × 2 selector = 754 UR callkey` (실측 — `ls index/by-callkey/
| grep -E '__0x(3593564c|24856bc3)\.json' | wc -l` = 754). 이 중 실제 (chain, UR주소)
배포 조합은 `universal-router` matrix 기준 **약 31 pair × 2 selector ≈ 62** 뿐.
→ **약 690 callkey 가 미배포 조합** (spurious).

**보안 도구 관점 심각도 평가 (냉정한 정량)**:
- spurious callkey 자체는 "그 주소로 그 selector tx 가 올 때만" 매칭. 정상 사용자는
  자기 체인의 실 UR 주소로만 보냄 → spurious callkey 는 **대부분 평생 dormant**.
- 위험 시나리오: 어떤 체인 X 에서 주소 A 가 (a) Uniswap UR 가 **아닌** 다른 contract
  인데 (b) 우연히 다른 체인의 UR 주소와 동일하고 (c) `execute` selector 를 노출.
  CREATE2 결정성 때문에 Uniswap UR 는 여러 체인에서 **같은 주소** → 이 경우 spurious
  callkey 가 가리키는 대상도 **대부분 진짜 UR** (예: `0x3fC91A3a…` = mainnet+Base
  v1.2, on-chain 확인 시 Optimism 에도 35919 byte UR 존재). 즉 cross-product 가
  "틀린 디코더" 를 적용할 확률은 낮음 — 같은 주소면 같은 opcode table 이 맞음.
- **실제 손해 경로**: 비-CREATE2 주소 (chain 고유 배포) 가 cross-product 로 무관
  체인에 노출 → 그 무관 체인의 동일 주소가 전혀 다른 protocol 이면 UR opcode table
  로 **오decode**. 단 `decode_with_json_abi` 가 `(bytes,bytes[],uint256)` ABI 로
  decode 실패하면 fault → static fallback (fail-safe). opcode stream 이 우연히
  ABI-valid 해야 오decode 성립 → 확률 낮으나 0 아님.

**판정** — 보안 도구로서 "spurious callkey = denial-of-wallet 표면 확대 + 저확률
mis-decode". P0/P1 아님 (자금 영향 없음, fail-safe 존재). **P2** — 정밀도 결함.
per-address split 을 UR 에도 적용하면 spurious 0 (다만 manifest 29개로 분리,
per_opcode_emit 중복). PoC 단계에서는 trade-off 수용 가능하나 **문서화 필요**.

**Remediation (택1)**:
1. UR 도 per-address split — per_opcode_emit 을 `$ref` 류로 외부화해 중복 제거.
2. cross-product 유지 시: WASM bridge lookup 후 **bundle `match` 와 실 (chain,to)
   재검증** 추가 — 단 현재 bundle `match` 자체가 cross-product 라 무의미. 진짜
   해법은 bundle 에 `chain→addr` 매핑 표현력 추가 (schema 확장).
3. 최소 조치: Tier B `is_uniswap_universal_router` allowlist 를 declarative UR
   dispatch 의 pre-check 로 재사용 (F-3 과 동시 해결).

---

### F-3 [P2] declarative UR opcode dispatch 에 `is_uniswap_universal_router` 게이트 부재

**Impact** — `crates/adapters/abi-resolver/src/subdecode/protocols/universal_router.rs`
는 `is_uniswap_universal_router(chain_id, target)` allowlist 함수를 제공하고, 그
doc-comment 가 명시: *"Selector match alone is not enough to safely dispatch — the
same selector is shared by every UR fork (Pancake, OKX, …)… Use
`is_uniswap_universal_router` in tandem so the orchestrator only applies
`UNISWAP_UR_TABLE` to addresses we trust."* 그러나 **declarative path
(`opcode_stream.rs`) 는 이 함수를 호출하지 않는다.** `dispatcher_id == "universal_router"`
면 `(chain,to)` 무관하게 곧장 `UNISWAP_UR_TABLE` 적용 (`opcode_stream.rs:134,173`).

**Root cause** — declarative UR dispatch 의 주소 신뢰는 전적으로 callkey 존재에 위임.
callkey 는 bundle `match.to[]` (29 주소) 의 cross-product → Tier B allowlist (31 pair)
와 **불일치**. Tier B 가 보유한 "(chain,addr) → UR 신뢰" 단일 진실이 declarative
경로에서 우회됨. CLAUDE.md 가 Tier B 를 "publisher 임의 주입 차단 위해 inner ABI 의
단일 진실 보유" 라 규정한 설계 원칙과 어긋남.

**exploit-works / exploit-fails 양면**:
- *works*: publisher 가 (혹은 본 generator 가) UR manifest `to[]` 에 비-UR 주소를
  넣으면 → 그 주소 callkey 가 UR opcode table 로 dispatch. 본 F-1 의 `0xde20…Cd9AF`
  가 정확히 이 경로 — 다만 빈 주소라 decode 자체가 실패해 fault→fallback (무해화).
  비어있지 않고 UR-아닌 contract 였다면 오decode.
- *fails*: `decode_with_json_abi` 가 `execute` ABI 로 decode 실패 시 fault →
  static fallback. opcode stream 이 ABI-valid 하지 않으면 차단됨.

**판정** — P2. F-2 와 동일 근본 (cross-product + allowlist 우회). 단독으로는
저확률이나 F-2 와 합쳐 "주소 신뢰 모델의 일관성 결함".

**Remediation** — `opcode_stream.rs` `dispatch_steps` 진입부에서
`ctx.chain_id` + `ctx.to` 로 `is_uniswap_universal_router` 호출, false 면
`MapperError::Unsupported` → static fallback. (Tier B 함수 이미 존재 — wiring 만 필요.)

---

### F-4 [P3] seed bundle chain 축소 — Base/Optimism/Arbitrum V2 swap 이 JIT 의존으로 전환

**Impact** — `browser-extension/public/seed-bundles/uniswap-v2-swapExactTokensForTokens
@1.0.0.json` 의 `match.chain_ids` 가 `[1,8453,10,42161]` → `[1]` 로 축소됨 (git diff
확인). seed bundle = Layer 1 (오프라인/registry 장애 시에도 동작하는 mounted bundle).
축소 후 Base/Optimism/Arbitrum 의 V2 `swapExactTokensForTokens` 는 **Layer 3 JIT
(registry fetch) 에만 의존** — registry 장애 시 이 3 체인 V2 swap 은 declarative 분석 불가.

**판정 — 이것은 버그 수정이지 회귀가 아님.** 기존 seed bundle 은 `chain_ids:[1,8453,
10,42161]` + `to:["0x7a250d56…"]` (mainnet V2Router02 1개) 였다. 그러나 V2 Router02
는 Base(`0x4752ba5D…`)·Optimism(`0x4A7b5Da6…`)·Arbitrum(`0x4752ba5D…`) 에서 **주소가
다름** (1차 출처 확인). 즉 기존 seed 는 비-mainnet 3 체인에서 **틀린 주소** 를 광고
하던 latent 버그였고, `[1]` 축소는 **정합화**다. per-address split 의 동일 원리.

**Remediation** — 기능적으로 정상. 보강하려면 Base/OP/ARB V2 swap 용 per-address
seed bundle 3개 추가 (Layer 1 오프라인 커버리지). PoC scope 상 우선순위 낮음 → P3.

---

### F-5 [P3] UR manifest `to[]` 에 v1.2 과거 버전 주소 혼입 — 의도적이나 미문서화

**Impact** — UR manifest `to[]` 29 주소는 chain 별 v1.2 + v2 + (Base 만) v2.1 의
union. `Uniswap/contracts` md 의 12 chain (Base 제외) 은 `### Universal Router`
**단일 섹션 (현 권장 1개)** 만 노출 — v1.2 과거 주소는 md Summary 에 없고 Deployment
History / Tier B table 에만 존재. 본 audit 의 cross-check: matrix UR 29 주소 전부
md body (Summary+History) 에 실재 확인 (F-1 의 1개 제외 시 28/28). on-chain `cast
code` 로 표본 (mainnet/Base/OP/ARB/Polygon/Unichain/BNB/World v1.2·v2) 전부 코드 존재
확인 — bytecode size 가 버전별 일관 (V1.2≈35919, V2≈39001, V2.1/일부≈43479).

**판정** — 주소는 **정확**. 단 "왜 한 chain 에 UR 주소 3개인가 (구버전 포함)" 가
manifest·매트릭스 주석에 없어, 미래 유지보수자가 "중복" 으로 오인해 삭제할 risk.
PHASE1_UNISWAP_RESEARCH.md §3.3 에는 설명 있음 — manifest 자체엔 없음. P3 (문서화).

**Remediation** — `uniswap-deployments.json` `universal-router` 각 배열에 버전 주석
(`// v1.2`, `// v2`, `// v2.1`) 추가. 기능 영향 없음.

---

### F-6 [P3] World Chain 에 `0x7a250d56…` 가 V2Router02 로 실재 — Tier B 수정의 잔여 혼동 표면

**Impact** — Tier B World Chain UR 수정 (`0x7a250d56…488D` → `0x03c4F6B5…`) 은
**정확하고 완전** (universal_router.rs 339-481행 table entry + 835-847행 test 둘 다
교체 — git diff 확인). 그러나 on-chain 검증 중 발견: World Chain (480) 에 `0x7a250d56
30B4cF539739dF2C5dAcb4c659F2488D` 는 **코드가 실재** (35919 byte). 이는 World Chain
의 UniswapV2Router02 — `uniswap-deployments.json` `v2-router02["480"]` 의 값은
`0x541aB7c31A119441eF3575F6973277DE0eF460bd` (md 일치) 인데, mainnet V2Router02 와
**동일 주소 `0x7a250d56…` 도 World Chain 에 별도 배포** 되어 있음.

**판정** — Tier B 수정은 옳다 (`0x7a250d56…` 는 UR 가 **아니다** — World Chain 에서도
V2Router02 이지 UR 아님). 다만 `0x7a250d56…` 가 World Chain 에 V2Router02 로 실재
한다는 사실은, 만약 누군가 World Chain V2 swap manifest 를 작성할 때 `0x541aB7c3…`
대신 `0x7a250d56…` 를 써도 **on-chain code 가 있어 오류가 안 드러남**. 잠재 혼동 표면.
matrix 의 `v2-router02["480"]` 는 `0x541aB7c3…` 로 **정확** (1차 출처 일치) — 현재
버그 아님. P3 (정보성 — 미래 risk 표면 기록).

**Remediation** — 조치 불요. `uniswap-deployments.json` `_comment` 또는
PHASE1 문서에 "World Chain 은 `0x7a250d56…` (mainnet 과 동일 V2Router02 CREATE2)
와 `0x541aB7c3…` 둘 다 V2Router02 — UR 아님" 1줄 기록 권장.

---

## 4. 정합 항목 (검증 통과 — Info)

### I-1 [Info] selector 정확성 — 6/6 통과

`cast sig` 로 모든 신규/관련 selector 재계산:

| 함수 | manifest selector | `cast sig` | 판정 |
|---|---|---|---|
| `execute(bytes,bytes[],uint256)` | `0x3593564c` | `0x3593564c` | OK |
| `execute(bytes,bytes[])` (no-deadline) | `0x24856bc3` | `0x24856bc3` | OK |
| `multicall(bytes[])` (multicall-bytes) | `0xac9650d8` | `0xac9650d8` | OK |
| `multicall(bytes32,bytes[])` (multicall-blockhash) | `0x1f0464d1` | `0x1f0464d1` | OK |
| `multicall(uint256,bytes[])` (기존) | `0x5ae401dc` | `0x5ae401dc` | OK |
| `permit(address,((address,uint160,uint48,uint48),address,uint256),bytes)` | `0x2b67b570` | `0x2b67b570` | OK |

`execute-no-deadline` 의 abi_fragment 는 `execute` 에서 `deadline` input 만 제거한
2-input (`commands`, `inputs`) — `gen-uniswap-ur.ts` 의 `inputs.length !== 2` 가드
정상. SR02 `multicall-bytes` = `multicall_recurse` 전략 + `recurse_rule_id:
self_array_bytes_last_arg`, `multicall-blockhash` 동일 — abi `[previousBlockhash:
bytes32, data: bytes[]]` 로 last-arg 가 `bytes[]` → recurse rule 정합.

### I-2 [Info] permit2/permit emit positional path 수정 — 정합 검증 완료

수정: `$.args.permitSingle.details.token` (nested-named) → `$.args.permitSingle[0][0]`
(positional). **3단계 cross-check 로 정합 확인**:

1. **eval.rs `walk_args` (211-351행)** — `name[idx][idx]…` chained **numeric** index
   지원, **dotted nested object (`$.args.x.y`) 미지원** (230행 doc 명시). → nested-named
   path 는 애초에 동작 불가, positional 만 유효.
2. **bridge.rs `convert_legacy_call` (68-80행)** — single-tuple flatten 조건 =
   `args.len() == 1 && Tuple && components 非empty`. `permit(owner, permitSingle,
   signature)` 는 **3-arg** → flatten **미적용**. 각 arg 는 `convert_arg` →
   `convert_value` (143-166행) 거침 → `DynSolValue::Tuple` → `DecodedValue::Tuple`.
3. **eval.rs `decoded_value_to_json` (43행)** — `DecodedValue::Tuple` → `serde_json::
   Value::Array` (JSON 배열). → `permitSingle` 는 JSON 배열. `permitSingle[0]` =
   `details` (배열), `[0][0]` = `token`. **positional path 가 정확히 token 을 가리킴.**

WASM declarative 경로 (`declarative_route_request_json` → `decode_with_json_abi`,
declarative_exports.rs:187) 도 동일 `convert_legacy_call` 사용 → 동일 결과. 수정 정합.

emit 의 다른 positional path 도 동일 원리로 검증:
`permitSingle[0][1]`=amount, `[0][2]`=expiration, `[1]`=spender, `[2]`=sigDeadline — 전부
struct layout `((token,amount,expiration,nonce), spender, sigDeadline)` 와 일치.

### per-address split 정합 (검증 통과)

- 표본 (`v2/swapExactTokensForTokens@1.0.0.json` = canonical mainnet,
  `swapExactTokensForTokens-optimism@1.0.0.json` = OP split): `emit` / `abi_fragment` /
  `requires` **byte-identical** (deep-copy 후 `match` 만 재작성 — `gen-uniswap-manifests.ts`
  198행 `JSON.parse(JSON.stringify(manifest))`). 원본 보존 확인.
- callkey collision: `ls index/by-callkey/ | sort | uniq -d` = **0** (filename 충돌 없음).
  build-index 가 1452 callkey 생성, collision 0.
- SR02 multicall overload 3종 (`multicall@5ae401dc` / `multicall-bytes@ac9650d8` /
  `multicall-blockhash@1f0464d1`) × 9 SR02 chain 의 address-group split (1/10/137/42161
  공유 `0x68b3…` + base/bnb/celo/unichain/avalanche 고유) 정합.
- non-UR 75 주소 (`v2-router02` 9 + `v3-swap-router` 4 + `swap-router-02` 9 +
  `v3-nfpm` 13 + `permit2` 13 + 표본 검증) — 1차 출처 `deployments/<chain>.md` 와
  **전부 일치**. on-chain `cast code` 표본 (Base/BNB/Unichain NFPM·SR02·V2,
  Blast/Celo/Avalanche/Zora) 전부 코드 존재.
- **참고 — benign md gap (버그 아님)**: `v3-swap-router["10"]` (Optimism V3
  SwapRouter01 `0xE592…`) 은 `deployments/10.md` 에 섹션 없음. 그러나 on-chain
  `cast call 0xE592… "factory()"` → `0x1F98431c8aD98523631AE4a59f267346ea31F984`
  (canonical Uniswap V3 Factory) 반환 → **실재하는 정확한 SwapRouter01**. md 의
  단순 누락. matrix 정확.

### Tier B 추가 table 검증 (통과)

`UNISWAP_UR_ADDRESSES` (21 entry) · `V3_NPM_ADDRESSES` (13 entry) 의 World Chain 외
나머지 항목 — PHASE1 조사 + 본 audit on-chain 표본 (UR v1.2/v2 8 chain, NFPM Base/
BNB/Zora) 에서 추가 오입력 없음. World Chain UR 수정만이 유일 변경 — 정확/완전.

---

## 5. 권고 우선순위

| # | severity | 조치 | 난이도 |
|---|---|---|---|
| F-1 | **P1** | `uniswap-deployments.json` OP UR `[10][2]` → `0xde20…d99af` 정정 + `gen-uniswap-ur.ts` 재실행 + `npm run build` | 낮음 (1줄 + 재생성) |
| F-3 | P2 | `opcode_stream.rs` dispatch 진입부에 `is_uniswap_universal_router(ctx.chain_id, ctx.to)` pre-check | 중간 |
| F-2 | P2 | UR per-address split (per_opcode_emit 외부화) **또는** F-3 으로 갈음 + spurious callkey 문서화 | 중간~높음 |
| F-4 | P3 | (선택) Base/OP/ARB V2 swap per-address seed bundle 추가 | 낮음 |
| F-5/F-6 | P3 | `uniswap-deployments.json` 주석 보강 (UR 버전 라벨 / World Chain V2Router02 노트) | 낮음 |
| 공통 | — | generator 에 "matrix 80 주소 전부 per-chain `eth_getCode != 0x`" 정합성 게이트 추가 — F-1 류 build 시 차단 | 중간 |

---

## 6. 출처

- Uniswap 배포 주소 (1차) — `Uniswap/contracts` repo `deployments/<chainId>.md`,
  13 chain (`1,8453,10,42161,137,43114,81457,56,42220,57073,130,480,7777777`) 전부
  `gh api repos/Uniswap/contracts/contents/deployments/<chain>.md` 로 fetch.
  - `deployments/10.md` Optimism UniversalRouter = `0xde20eee5398d3790a4d356e8925bd21ea65d99af`
    (Summary table + Deployment History + heading 3곳 일치) — F-1 의 1차 근거.
  - `deployments/480.md` World Chain UniversalRouter = `0x03c4f6b55733cdf3caa07c01e5b83ddee3381f60`
    (단일 — v1.2 분리 배포 없음) — Tier B 수정 검증 근거.
- on-chain 검증 — `cast code` / `cast call` (foundry cast 1.5.1):
  - Optimism `mainnet.optimism.io`, mainnet `eth.llamarpc.com`, Base `mainnet.base.org`,
    BNB `bsc-dataseed.binance.org`, Polygon `polygon-bor-rpc.publicnode.com`,
    Arbitrum `arb1.arbitrum.io/rpc`, Unichain `mainnet.unichain.org`,
    World Chain `worldchain-mainnet.g.alchemy.com/public`, Avalanche `api.avax.network`,
    Blast `rpc.blast.io`, Celo `forno.celo.org`, Zora `rpc.zora.energy`.
  - `0xde20…Cd9AF` (matrix OP UR) = Optimism/mainnet/Base 모두 코드 없음.
  - `0xde20…d99af` (md OP UR) = Optimism 43479 byte.
  - `0xE592…` Optimism `factory()` → `0x1F98431c8aD98523631AE4a59f267346ea31F984`.
- selector — `cast sig "<canonicalSignature>"` (keccak256[:4]), 6/6 검증.
- ScopeBall 코드 — `crates/adapters/abi-resolver/src/subdecode/protocols/universal_router.rs`,
  `crates/adapters/abi-resolver/src/bridge.rs`, `crates/adapters/mappers/src/declarative/
  {eval,opcode_stream}.rs`, `crates/policy-engine-wasm/src/declarative_exports.rs`,
  `registry/scripts/{gen-uniswap-manifests,gen-uniswap-ur,build-index}.ts`.
- 본 audit 대상 산출물 — `registry/scripts/uniswap-deployments.json`,
  `registry/manifests/uniswap/` (324 per-address manifest + UR 2),
  `registry/index/by-callkey/` (1452 callkey).
- 작업 맥락 — `PHASE1_UNISWAP_RESEARCH.md` (Phase 1 조사 — §4.1 World Chain 버그 사전 식별).
