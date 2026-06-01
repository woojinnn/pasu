<!-- ─────────────────────────────────────────────────────────────────────────
  이 문서는 AI 에이전트(Claude Code / Codex 등)가 읽고 그대로 실행하는 매뉴얼이다.
  새 EVM 프로토콜 요청 → 어댑터 전수 작성 → 실거래 디코드 정확성 검증 → 수정 루프.
  대상 경로: V3 ActionBody[] 디코드 중심. 레거시(V1 ActionEnvelope)는 이미 제거됨.
  실제 verdict path 는 이후 lowering_v2 → per-policy schema/Cedar evaluation 으로 이어지므로 새 action/domain/live field 는 downstream policy contract 까지 포함.
  본 파일은 crates/integration-tests/*.md tracking 정책에 포함되는 온보딩 문서다.
  마지막 grounding: 2026-05-31 (Lido liquid_staking 온보딩 + enrichment §4d + I0 contract-inventory gate + Permission domain 반영). file:line·도메인 카운트는 작성 시점 기준 — 항상 grep 재확인(코드/도메인 둘 다 늘어난다).
───────────────────────────────────────────────────────────────────────── -->

# ScopeBall — 신규 프로토콜 온보딩 & V3 디코드 테스트 매뉴얼

> **독자 = AI 에이전트.** 이 문서 하나로 새 프로토콜을 온보딩(어댑터 전수 작성)하고, 실거래로 `ActionBody[]` 디코드 정확성을 검증하고, gap 을 고치는 루프를 돌 수 있어야 한다. 세션 컨텍스트 없이도 동작하도록 모든 경로·커맨드·포맷을 embed 했다.

> **📂 인스트럭션 문서 맵** (전부 `crates/integration-tests/`, gitignore 제외=tracked):
> - **`PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md`** — protocol-agnostic completion model + semantic oracle contract + strict audit skeleton. 새 프로토콜 작업 전 먼저 읽는다.
> - **이 파일 = spine** — P0~P4 전체 방법론 (research → author → test → develop → land).
> - **`README.md`** — 하니스 runbook (CLI · 3 입력소스 · Log→Gap→Develop 루프). P2~P4 운용.
> - **`ACTIONBODY_EXTENSION_GUIDE.md`** — Tier 3 ActionBody Rust/Cedar 확장 (새 domain/action/live_field). §4a·§4d 에서 진입.
> - **`registryV2/surface/README.md`** — surface gate(I0/I1) ops + `_deployments.json` 포맷. **gate 데이터 옆에 둠**(co-located, script 가 참조) — 위치만 예외.
> - **`ONBOARDING_PROMPT.md`** — 새 세션 kickoff 프롬프트(복붙용; 워크플로·게이트·가드레일 embed). `<PROTOCOL>` 만 바꿔 새 세션에 입력.
>
> **♻️ 재진입(idempotent)**: 이 플로우는 **이미 온보딩된 프로토콜을 input 으로 넣어도 동일하게** 돈다 — greenfield 전제 아님. P0 에서 현 `surface/_deployments`·`coverage`·manifest 와 **1차 출처를 다시 diff** → 틀린 곳 수정·빠진 곳 보충, P2 에서 현 corpus 회귀 + 신규 gap 추가. "처음 온보딩" 과 "기존 재검증·보강" 이 같은 커맨드·게이트(§7 check:surface/manifest + corpus)로 수렴한다(§6 루프가 그 엔진).

---

## 0. 목적 · 범위 · 비범위

ScopeBall = EVM 권한 위임을 **서명 직전**에 정적 분석하는 브라우저 익스텐션. raw Tx(calldata) 를 받아 **어떤 어댑터(manifest)를 쓸지 정하고 → 디코드 → 매핑 → `ActionBody[]`** 로 정규화한다. 이 매뉴얼이 다루는 것은 그 **디코드 경로(raw Tx → ActionBody[])의 정확성**이다.

### 이 매뉴얼이 다루는 것
- **단 하나의 경로: V3 ActionBody 경로.** 진입 = WASM export `declarative_route_request_v3_json`. 산출 = `Vec<ActionBody>`, 이후 policy verdict path 로 전달된다.
- 새 프로토콜의 진입점 함수에 대한 **어댑터(Tier A manifest) 전수 작성**, 필요 시 **generic 엔진 확장(V3 Tier B)** 과 **ActionBody 스키마 확장(Tier 3)**.
- **실거래 기반 디코드 정확성 검증** — 합성(synthetic) + 실거래(Etherscan/Dune) 두 입력.

### 이 매뉴얼이 다루지 **않는** 것 (명시적 비범위)
- **레거시 V1 ActionEnvelope 경로** — `feat(v2)! retire legacy v1 ActionEnvelope path` (commit `4e60392`) 로 **이미 제거됨**. `mapper::DeclarativeMapper`, `single_emit.rs`, `opcode_stream.rs`, `eval.rs`, `enum_tagged.rs`, `multicall.rs`, `array_emit.rs`, `builtin_fn.rs` 는 더 이상 존재하지 않는다. declarative 레이어는 **이제 V3-only by construction** 이다 — V1/V3 혼동을 할 필요가 없다.
- **`live_inputs` *값*의 실제 RPC 채움** — live_field 의 **값**은 host RPC 서버가 production 에서 채운다(테스트는 **default 스텁** — 빈 값 / chain별 정적 주입만). 이 매뉴얼의 검증은 calldata→ActionBody **정적 디코드**만 본다.
  - ⚠️ **그러나 "이 action 이 *어떤* live_field 를 가지는가" 는 비범위가 아니다 — P1 author 의 설계 결정(§4d ENRICHMENT)이다.** 값 채움(host·런타임)과 필드 설계(author·작성 시점)는 별개다. 디코드된 필드가 추상/불투명 단위(shares, 내부 index, wrapped 수량)면 환산 live_field 없이는 디코드가 성공해도 사용자는 의미를 못 본다. "테스트가 빈 스텁을 쓴다" ≠ "manifest 의 `live_inputs` 를 비워도 된다".
- **Cedar 정책 의미 자체 / 시뮬레이션 / SW 메시징 UX** — 디코드 하류. 단, 새 Tier3 action/domain/live field 가 생기면 `lowering_v2`, per-policy schema, Cedar action/resource schema 등록은 온보딩 범위다. 그렇지 않으면 production verdict path 에서 `MissingAction`/schema mismatch 로 멈춘다.

### 핵심 원칙 4가지 (먼저 내면화)
1. **테스트는 게이트가 아니라 루프 엔진**이다. manifest 0개로 돌리면 전부 `uncovered` → 그게 곧 "무엇을 작성할지" 지도(=discovery). 작성 후 돌리면 정확성 검증(=accuracy). **같은 하니스, 같은 실거래 데이터.**
2. **정답(oracle) 작성량은 tx 수가 아니라 selector 수에 비례**한다. 10,000 tx 든 1,000 tx 든 작성할 정답은 selector 수(~수십)만큼. tx 를 더 돌리는 건 거의 공짜이나 shape 가 포화되면 marginal 가치가 급감.
3. **막히는 경우는 없다.** 매핑 불가 함수는 `ActionBody::Unknown`(warn/deny)으로 떨어진다 → 디코드가 멈추지 않음. 단 **존재하지 않는 domain/action 을 target 하는 manifest 는 hard-fail** 한다(아래 4a).
4. **모든 사실 진술은 1차 출처.** 컨트랙트 주소/ABI 는 공식 docs · Sourcify · Etherscan/BaseScan verified 페이지에서만. 추측 주소 금지.

### 0.1 Action model preflight — `crates/simulation/reducer/src/action` 에 없는 의미

`crates/simulation/reducer/src/action` 은 **프로토콜 목록이 아니라 protocol-agnostic intent catalog** 다. 예를 들어 Uniswap 전용 `ActionBody::Uniswap` 이 없어도 Uniswap swap 은 `amm`, Permit2 는 `permission`/`token`, router nesting 은 `multicall` 로 매핑된다. 따라서 "프로토콜이 action dir 에 없다" 는 그 자체로 문제가 아니다. 문제는 **그 프로토콜의 user-facing 함수 의미를 기존 domain/action 이 정확히 표현할 수 있느냐**다.

P0 리서치로 COVER selector 를 찾은 직후, P1 manifest 작성 전에 아래 gate 를 먼저 통과한다:

```text
COVER selector 의미가 기존 ActionBody 로 표현됨?
  YES → 기존 domain/action 으로 P1 manifest 작성
  NO, fund-move/permission/user-risk 의미임 → Tier 3 ActionBody 확장 선행
  NO, user-facing safety 의미가 낮거나 opaque/admin/keeper임 → EXCLUDE 또는 Unknown, reason 필수
```

Tier 3 선행이 필요한 경우 순서:
1. **normalized intent spec 작성** — user 가 서명 직전에 알아야 하는 의미를 protocol-independent field 로 정의. 예: authorizer, spender/operator, asset, amount, recipient, market/pool, expiry, grant/revoke flag, live conversion source.
2. **기존 domain 확장 우선** — 새 protocol 전용 domain 을 만들기 전에 `token`/`amm`/`lending`/`permission`/`staking` 등 기존 domain 의 새 action 으로 충분한지 검토.
3. **새 domain 은 마지막 선택** — 기존 domain 으로 의미가 왜곡되거나 정책/UX/위험 모델이 완전히 다를 때만 추가.
4. **권한 grant 는 Unknown 금지** — `approve/permit/authorize/delegate/operator/allow` 류는 맞는 Action 이 없으면 Tier 3 를 추가한다. ScopeBall 핵심 surface 라서 "표현 안 됨"을 이유로 skip 하지 않는다.
5. **`ACTIONBODY_EXTENSION_GUIDE.md` 먼저 실행** — Rust ActionBody, reducer/view, VALID_DOMAINS, Cedar/lowering, TS/export, conformance/golden 를 동기화한 뒤 manifest 를 작성한다.

즉 P0 자체는 여전히 먼저 돈다. 다만 COVER selector 의 의미가 현재 Action catalog 에 없으면 **P1 manifest 보다 Tier 3 schema design/extension 이 선행**된다. 존재하지 않는 domain/action 을 manifest 가 target 하면 production decoder 가 hard-fail 하므로 schema-first 가 안전하다.

---

## 1. 코드베이스 지도 (V3 hot path)

raw Tx 한 건이 `ActionBody[]` 가 되기까지 호출되는 실제 코드. **이 표의 좌표를 기준으로 작업**하되, 작성/수정 전 항상 `grep -n` 으로 라인 재확인(코드가 움직인다).

### 1.1 경로 다이어그램

```
raw Tx { chain, to, selector, calldata, value }
  │
  ▼  declarative_install_v3_json(bundle_json)            policy-engine-wasm/src/declarative_exports.rs:177
     └─ thread-local DECLARATIVE_V3_STATE 에 bundle 설치  (declarative_exports.rs:134)
  │
  ▼  declarative_route_request_v3_json(input_json)       declarative_exports.rs:329  ★단일 오케스트레이터(~737)
     ├─ 1) callkey lookup  (chain,to,selector) → bundle  (DECLARATIVE_V3_STATE.bridge)
     ├─ 2) DECODE  decode_with_json_abi(abi_fragment, calldata)
     │        abi-resolver/src/bridge.rs:293 → decode.rs:71 decode_with_function  = alloy 디코드
     │        → args_json::args_to_json(decoded)          mappers/.../args_json.rs:55  → args_json
     ├─ 3) resolved 정적 주입 (WETH / V4 pool_manager)    declarative_exports.rs ~480-513  (= live_inputs default)
     ├─ 4) DISPATCH  match strategy.as_str()              declarative_exports.rs:530   ★분기
     │        "single_emit"           → build_action_body                  action_builder.rs:514
     │        "opcode_stream_dispatch" → build_multicall_from_opcode_stream action_builder.rs:966
     │        "array_emit"            → build_array_emit                    action_builder.rs:1048
     │        "tagged_dispatch"       → build_tagged_dispatch               declarative_exports.rs(로컬)
     │        "multicall_recurse"     → build_multicall_recurse_body        declarative_exports.rs:1826
     │        그 외                    → error "unsupported_strategy"
     │        (placeholder 해소 = action_builder.rs substitute_placeholders:238 / walk_json_path)
     ▼
  Vec<ActionBody>   (serde tag="domain")                  simulation/reducer/src/action/mod.rs:141
```

### 1.2 파일 reference 표

| 역할 | 파일 | 핵심 심볼 |
|---|---|---|
| **진입 / install** | `crates/policy-engine-wasm/src/declarative_exports.rs` | `declarative_install_v3_json`(177), `declarative_route_request_v3_json`(329), dispatch `match strategy`(530) |
| **입력 DTO** | `crates/policy-engine-wasm/src/dto.rs` | `DeclarativeRouteRequestV3InputDto`(198-227) |
| **decode (alloy)** | `crates/adapters/abi-resolver/src/bridge.rs`, `.../decode.rs` | `decode_with_json_abi`(bridge:293) → `decode_with_function`(decode:71) |
| **args→JSON** | `crates/adapters/mappers/src/declarative/args_json.rs` | `args_to_json`(55), `decoded_value_to_json`(25), `decoded_value_to_json_typed`(71) |
| **V3 generic 엔진** | `crates/adapters/mappers/src/declarative/action_builder.rs` | `build_action_body`(514), `build_multicall_from_opcode_stream`(966), `build_array_emit`(1048), `substitute_placeholders`(238), 내부 `resolve_placeholder`/`walk_json_path`/`flatten_body` |
| **Bundle JSON 타입** | `crates/adapters/mappers/src/declarative/types.rs` | `BundleMatch` 등 |
| **ActionBody 스키마(Tier 3)** | `crates/simulation/reducer/src/action/**` | `ActionBody`(mod.rs:141) + domain sub-enum |
| **하니스 진입** | `crates/integration-tests/src/harness/{adapters,route,oracle,corpus}.rs` | `load_and_install`, `route_calldata`, `judge`, `run_corpus` |
| **하니스 CLI** | `crates/integration-tests/src/bin/v3_harness.rs` | fuzz/coverage/replay/corpus/import-* |
| **게이트 테스트** | `crates/integration-tests/tests/v3_decode_harness.rs` | 4 test |
| **manifest(어댑터)** | `registryV2/manifests/<publisher>/<contract>/<func>@<ver>.json` | schema_version "3" |
| **index 빌드** | `registryV2/scripts/build-index.ts` | callkey 생성 |

### 1.3 3-tier 구조 (어디를 건드리나)

| Tier | 무엇 | 위치 | 변경 무게 |
|---|---|---|---|
| **Tier 1 — manifest** | abi_fragment + emit (선언형 JSON) | `registryV2/manifests/` | 가벼움 (JSON only, release PR 불필요) |
| **Tier 2 — generic 엔진** | placeholder/strategy 해석기 | `action_builder.rs` (+ `declarative_exports.rs` dispatch) | 무거움 (Rust, WASM 재빌드) |
| **Tier 3 — ActionBody 스키마** | 디코드 결과 타입(=정규화 산출) | `crates/simulation/reducer/src/action/` | 가장 무거움 (Cedar/TS/oracle 동기화) |

> **주의:** Tier 2 는 `subdecode/protocols/*.rs`(V1 의 opcode 테이블)가 **아니다.** V3 경로의 opcode/array/nested 전개는 `action_builder.rs` 의 generic 빌더 + manifest 의 `per_opcode_body` 가 담당한다. (subdecode 는 V1 잔재 / 일부 inner-decode 헬퍼.)

---

## 2. 전체 E2E 플로우

```
[입력: 새 프로토콜]
   │
   P0 RESEARCH ── 1차출처로 canonical 컨트랙트 + 주소 + 체인 + token-surface 인벤토리
   │
   P1 AUTHOR  (함수마다, schema → manifest → engine → enrich 순)
   │   ├ Tier3 schema  : 함수가 어느 ActionBody variant 로 매핑? 없으면 추가 or Unknown
   │   ├ Tier1 manifest: registryV2/manifests/<p>/ — abi_fragment + emit(strategy)
   │   ├ Tier2 engine  : generic 빌더로 표현 안 되는 shape 만 action_builder.rs 확장
   │   ├ tokens        : registryV2/tokens/<chain>/<addr>.json 등록/보강
   │   └ ENRICH (§4d) : 디코드 필드가 user-legible 한가? 추상 단위(shares/index/wrapped/
   │                    rate-dependent)면 환산 live_field 추가 (없으면 사유 명시 defer)
   │        ▲
   │        │  gap 분류 → 처치 → 재테스트
   P2 TEST ⇄┘  install_v3 → route_request_v3 → ActionBody[]   (live_inputs 값 = default 스텁)
   │   ├ _synthetic : 어댑터 기반 calldata 합성 + edge → 입력값 ↔ ActionBody 필드 inverse-check
   │   ├ real-tx    : 어댑터 BLIND 무작위 pull → hybrid oracle 로 정확성/커버리지 버킷
   │   └ → logs/<p>/ 기록
   │
   P3 DEVELOP ── 로그 gap 분류 → manifest 수정 or 엔진 확장 or schema 추가 → 회귀
   │
   P4 LAND ── build-index → build-index vitest → check:manifest(CI-safe representative index + source-ref representative) → check:surface → check:universe(pool/factory/vault-heavy) → coverage/fuzz/corpus → targeted/full manifest gate as feasible → cargo test --workspace 0 fail → fmt/clippy → commit
```

**P1 ↔ P2 는 루프다.** 처음엔 TEST 가 "무엇을 작성할지"(discovery), 나중엔 "정확한가"(accuracy). 작은/단순 프로토콜은 ABI 만 보고 P1 부터 시작해도 되고, 크고 복잡한 프로토콜(UR류, batch)은 P2 를 먼저 한 번 돌려 실제 selector/shape 를 보면 rework 가 준다(선택).

**P1 직후 자가검증 — `npm run check:manifest` (emit.body shape build 강제).** manifest 의 `emit.body` 는 Tier 3 `ActionBody` struct 와 **정확히** 일치해야 한다(필드명·variant·venue/param shape·필수 `live_inputs`). 그런데 build-index 는 pass-through라 이 일치를 검사하지 않아서, 예전엔 틀린 shape 를 **decode 테스트가 실패해야** 비로소 알았다(예: `build_action_body_failed: missing field live_inputs`) — 프레임워크가 "한 큐"로 안 돌고 author 가 decode-error 를 보고서야 shape 를 역추정. 이제는 author 직후 `npm run check:manifest`(= `build-index --summary-only --representative-source-refs` → `v3-harness validate --representative-source-refs`) **한 번**이면, production 디코더로 type-valid 입력을 합성·라우팅해 `emit.body` 가 안 맞는 manifest 를 **bundle id + 정확한 필드 오류 + repro 커맨드**와 함께 exit 1 로 잡는다. 이 기본 gate 는 source-materialized protocol 의 index 폭증/OOM 을 피하려고 **같은 source-ref bundle template 당 1개 callkey 만 materialize/install**한다. 특정 프로토콜만 빠르게: `cargo run --bin v3-harness -- validate --filter <protocol> --representative-source-refs`. 전수 검증이 필요하거나 nightly/local 장비에서 여유가 있으면 `npm run check:manifest:full`(= full build-index → exhaustive `v3-harness validate`) 를 별도 실행하고 evidence 에 결과 또는 resource blocker 를 남긴다. input-의존 아티팩트(`value-map: no case`, array OOB)는 oracle-soft 라 `$args.i` fuzz 가 coin index 범위를 벗어나도 false-positive 안 난다. **이게 §3 의 `check:surface`(research 전수성)와 같은 패턴 — authoring 정확성을 agent 의 trust 에서 build-enforced invariant 로 승격**(틀린 shape = "안 보임"이 아니라 build 실패). 한계: 현재 `single_emit` 전략 한정 — `array_emit`/`opcode_stream`/`typed_data` 는 `fuzz`/`corpus` 가 커버.

### 2.1 작업 워크플로 (worktree · phase 커밋 · sub-agent)

온보딩은 양이 크고 다단계다. 아래 운영 규약을 따른다:

1. **요청받은 worktree 에 전용 브랜치를 먼저 만든다.** 사용자가 프로토콜 온보딩용 worktree/cwd 를 지정하면 그 worktree 안에서 `git switch -c feat/<protocol>-onboarding`(이미 있으면 `git switch feat/<protocol>-onboarding`) 후 작업한다. 사용자가 별도 worktree 를 지정하지 않았을 때만 `git worktree add -b feat/<protocol>-onboarding ../<dir> <base>` 로 **격리된 worktree + 브랜치**를 만든다. base/다른 worktree 가 점유·dirty 면 그 worktree 는 비접촉. 온보딩 프레임워크가 브랜치에서 완료되어도 **base/worktree 머지는 사용자가 명시적으로 요청할 때만** 진행한다.
2. **phase 끝나면 커밋.** P0/P1/P2/P3/P4 각 phase(또는 더 잘게 — 컨트랙트별·함수군별)가 끝날 때마다 **explicit-stage 커밋**(`git add <파일>`, `git add -A` 금지). 중간 유실 방지 + reviewable history + 회귀 지점. 메시지 말미 `Co-Authored-By`.
3. **한 큐로 진행한다.** 브랜치·외부 데이터 lane 이 준비되면 P0→P4 를 phase 경계에서 확인 요청 없이 이어서 수행한다. phase 종료 커밋은 체크포인트일 뿐 멈춤 지점이 아니다. 커밋 후 곧바로 다음 phase 로 넘어간다. 멈추는 경우는 (a) 사용자 승인 없이는 할 수 없는 merge/push/destructive action, (b) Etherscan/Dune/auth 같은 외부 의존성 부재, (c) 1차 출처로도 해소 불가능한 스코프 모호성, (d) 같은 blocker 3회 이상 반복뿐이다.
4. **sub-agent 적극 활용.** 한 세션에 다 담기엔 양이 크다 → **fan-out 가능한 작업은 sub-agent 로 분할**: P0 컨트랙트별 research/discovery, P0 token-surface research(`crates/integration-tests/TOKEN_INVENTORY_GUIDE.md`), P1 함수(selector)별 manifest 작성(authoring ∝ selector, §2), P1 Tier3/lowering/cedarschema review, P2 synthetic edge matrix, P2 소스별(Etherscan/Dune) corpus pull+convert, P3 gap triage, surface snapshot per-contract. 메인 세션은 **종합·검증·게이트·커밋**을 맡는다.
5. **중요 구간은 Claude Code 2nd-opinion 을 쓴다.** P0 contract/token discovery 는 필수. 그 외에도 (a) 새 domain/action/live_field 를 추가하는 Tier3 설계, (b) 권한 grant·fund movement selector 매핑, (c) P2 synthetic edge-case 설계, (d) P2 real-tx corpus verdict 분류, (e) 반복되는 hard decoder gap 의 root-cause triage 는 Claude Code 또는 독립 sub-agent 에 같은 입력을 주고 결과를 합친다.
6. **sub-agent 프롬프트는 self-contained·디테일하게.** sub-agent 는 **이 세션의 컨텍스트가 없다** → 프롬프트에 (a) repo·branch·cwd·worktree 경로 (b) 현재 목표/phase 와 non-goal (c) 읽을 인스트럭션 문서 (d) 정확한 대상 파일·심볼·좌표 + **미러할 기존 선례**(예: "`lending::supply` 전 경로 복제") (e) 정확한 산출물·출력 포맷·통과할 게이트 (f) 가드레일(explicit-stage·1차출처·무관 churn 금지·수정 권한 범위)을 **전부 embed**. 면밀할수록 rework 가 준다(fresh-PC self-contained 원칙과 동형).
7. **sub-agent 결과는 candidate-only 다.** 메인 세션이 반드시 (a) 실제 코드/문서/1차 출처와 대조, (b) Codex 결과와 sub-agent 결과의 diff 정리, (c) 불일치 항목의 disposition(accept/drop/defer + 이유), (d) build/test gate 로 검증을 수행한다. sub-agent 산출물을 검증 없이 복사하거나 커밋하지 않는다.
8. **증거 ledger 없으면 완료가 아니다.** `crates/integration-tests/ONBOARDING_EVIDENCE_TEMPLATE.md` 를 `crates/integration-tests/onboarding/<protocol>/evidence.md` 로 복사해 채운다. P0/P1/P2/P3/P4 각 mandatory row 가 `done` 또는 구체적 `blocked` 가 아니면 해당 phase 를 완료로 말하지 않는다. 템플릿 row 를 삭제해서 통과시키면 안 된다; `check-onboarding-evidence` 는 템플릿의 필수 row 존재 여부도 검사한다. 완료 선언 전 `cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <protocol> --phase <p0|p1|p2|p3|p4|all>` 을 실행해 대상 phase gate 가 PASS 해야 한다. 사용자가 "했냐?"라고 물었을 때 "안 했습니다"가 아니라, ledger 의 명령·결과·카운트·blocker 로 답해야 한다.

### 2.1a 외부 데이터 도구 요구사항 (P2 real-tx)

P2 real-tx 는 실제 on-chain corpus 로 검증하는 단계라서 로컬 코드만으로 완료 판정할 수 없다.

| 도구 | 판정 | 용도 |
|---|---|---|
| **Etherscan API 또는 Etherscan MCP** | **필수** | txlist bulk lane, verified ABI/source fallback, address labels. `.env` 의 `ETHERSCAN_API_KEY` 사용. 키·raw dump 는 commit 금지 |
| **Dune MCP/API** | **필수에 가까운 gap lane** | Base/OP 등 free Etherscan txlist 미지원 체인, decoded namespace/table discovery, selector frequency, cross-chain cohort, long-tail contract/token discovery |
| **Sourcify API** | 강력 권장 | verified contract ABI/source cross-check. API key 없이 1차 검증 보조 가능 |
| **GitHub / Browser / sub-agent** | 강력 권장 | 공식 deploy artifact, protocol repo, local extension/runtime 확인, 독립 검토 fan-out |

연결 힌트(키는 로컬 설정만, repo commit 금지): Dune remote MCP URL = `https://api.dune.com/mcp/v1`(OAuth 또는 `x-dune-api-key`), Etherscan remote MCP URL = `https://mcp.etherscan.io/mcp`(`Authorization: Bearer <ETHERSCAN_API_KEY>`). MCP 가 없으면 직접 REST API 로도 가능하지만, 온보딩 세션은 도구 연결 여부를 phase 시작 시 기록한다.

둘 중 하나라도 연결이 없으면 authoring/P2 synthetic 까지는 진행할 수 있지만 **P2 real-tx complete 를 선언하지 않는다**. 그 경우 phase 로그에 `blocked_external_data: etherscan|dune`, 수행한 synthetic/golden 범위, 후속 연결 후 재실행할 주소·selector 목록을 남긴다.

### 2.1b 완료 선언 금지 조건

아래 중 하나라도 비어 있으면 "완료"라는 단어를 쓰지 않는다. "부분 완료" 또는 "blocked" 로 말하고 계속 진행 가능한 다음 작업을 수행한다.

| phase | 완료 전 필수 증거 |
|---|---|
| P0 research | Claude Code/sub-agent exact command or agent id, result summary, Codex-only candidates, Claude-only candidates, dropped-unverified candidates, final first-party disposition, pool/factory address-universe source/query/count + cover/exclude/defer disposition if applicable, `check:surface` output, `check:universe -- --protocol <protocol>` output for pool/factory/vault-heavy protocols |
| P1 author | per-COVER selector ActionBody/Tier3 mapping, permission/fund-movement red-flag review, manifest file list, enrichment/live_field decision, required remote policy-RPC/live/enrichment method local-handler/configured-endpoint/blocker disposition, Tier3 downstream files/tests if applicable, CI-safe `check:manifest` or protocol-filtered `validate --representative-source-refs` output, exhaustive `check:manifest:full` result or resource blocker if source-ref surface is huge |
| P2 synthetic | fuzz seed/iteration command, fixed edge matrix, pass/error corpus disposition |
| P2 real-tx Etherscan | MCP/API availability, adapter-blind txlist command/query, api_calls_used, raw_txs_seen, unique_selectors_seen, per-COVER-selector sample coverage, pool/factory candidate-universe sweep if applicable, unknown_protocol_address gaps, representative corpus/golden disposition |
| P2 real-tx Dune | MCP/API availability, usage baseline, query id/SQL summary with partition WHERE, rows returned, executionCostCredits or usage delta, selected tx hashes or explicit blocker; for pool-heavy/factory protocols, Dune selector/address stats over candidate universe when Etherscan cannot cover it |
| P3 develop | every gap bucketed including unknown_protocol_address, each fix tied to gap id/selector/tx/seed, P2 rerun output after fix, corpus expect flips/exclusions justified, remaining gaps dispositioned |
| P4 final | exact changed files, build-index vitest/check:manifest(CI-safe)/check:manifest:full(or resource blocker)/check:surface/check:universe/v3-harness/cargo/wasm/fmt-clippy-typecheck outputs as applicable, `check-onboarding-evidence --phase all` pass output, staged file list, commit hash, remaining WARNs, explicit deferred selectors/actions with reason, no merge unless user requested |

### 2.2 sub-agent / Claude Code orchestration

| phase | 분할하기 좋은 작업 | 독립 검토/병합 규칙 |
|---|---|---|
| P0 contract | contract/deployment discovery, pool/factory address-universe discovery, ABI/surface snapshot, coverage triage | Codex ∪ Claude ∪ 공식 deploy/pool list 를 합친 뒤 1차 출처 + `check:surface` 로 dispose. pool-heavy 프로토콜은 universe 를 먼저 닫고 그 뒤 cover/exclude/defer |
| P0 token | LP/share/receipt/debt/governance/base token inventory, underlying ref 확인 | `crates/integration-tests/TOKEN_INVENTORY_GUIDE.md` 기준. token JSON 은 ERC 표준 callkey 입력이므로 누락 후보를 P2 token tx miss 와 대조 |
| P1 selector | selector batch 별 ActionBody mapping, manifest emit, permission red-flag scan | 권한 grant/fund movement 는 Claude Code 2nd-opinion 권장. mapping 이유와 skipped side-effect 를 manifest note 로 남김 |
| P1 Tier3 | 새 action/domain/live_field 설계, lowering_v2, cedarschema, schema registration | 구현 전 독립 설계 리뷰. `ActionBody → lowering_v2 → cedarschema` 필드명/타입/action uid diff 를 반드시 검증 |
| P2 synthetic | fuzz seed, edge-case matrix(permission/value/nested/array/opcode/deadline/path bytes) | sub-agent 가 제안한 edge 를 메인 세션이 dedup 후 corpus/fixture 로 편입. generated-only shape 에 치우치지 않는지 확인 |
| P2 real tx | Etherscan/Dune pull, verdict bucketing, representative corpus extraction | source별 tx selection 은 adapter-blind. verdict 는 hard/soft/mis_decoded/excluded 로 메인 세션이 재분류 |
| P3 gaps | hard decoder failure root cause, Tier1/Tier2/Tier3 fix 후보 | 같은 failure 에 대해 agent 간 원인 분류가 다르면 재현 커맨드와 최소 fixture 로 결정 |
| P4 audit | final surface/manifest/corpus diff, stale docs/checklist review | 다른 agent 에 "무엇이 빠졌나?"만 묻고, 메인 세션이 실제 gate 로 검증 |

**Claude Code prompt skeleton**:

```text
Repo: /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2
Branch/worktree: <branch>, <worktree path>
Phase: <P0 contract | P0 token | P1 selector | P1 Tier3 | P2 synthetic | P2 real-tx | P3 gap | P4 audit>
Goal: <one concrete deliverable>
Non-goals: <files/areas not to touch>
Read first:
- crates/integration-tests/PROTOCOL_ONBOARDING_AND_TESTING.md
- crates/integration-tests/PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md
- crates/integration-tests/ACTIONBODY_EXTENSION_GUIDE.md (if Tier3)
- crates/integration-tests/TOKEN_INVENTORY_GUIDE.md (if token inventory)
Context:
- Protocol/chains/contracts/selectors/tx hashes in scope: ...
- Existing analogous implementation to mirror: ...
Output required:
- Findings table with source/file references
- Proposed file edits or artifact list
- Open risks / unverified candidates
- Commands/gates to run
Rules:
- Do not trust LLM memory. Use first-party source or local code.
- Do not touch unrelated files.
- Do not commit.
- Mark uncertain items as unverified, not facts.
```

---

## 3. P0 — RESEARCH (1차 출처 인벤토리)

### 산출물
프로토콜의 **external state-changing 함수 전수 + per-function triage 표**. "우리가 이미 manifest 가진 것" 이나 "눈에 띄는 deposit/withdraw 몇 개" 가 아니라 **hub 컨트랙트의 external 함수를 하나도 빠짐없이 나열**(예: Balancer Vault, Aave Pool+Gateway, Uniswap UR/Routers/NFPM/Permit2, Morpho singleton). 각 함수를 `COVER` 또는 `EXCLUDE: <reason>` 로 **명시 분류** — 누락은 "안 봄" 이 아니라 **버그**. (이 단계가 부실하면 가장 중요한 함수를 silent 하게 빠뜨린다 — §9 의 Morpho `setAuthorization`(권한 위임) 1차 누락이 그 사례.) **이 전수성은 이제 산문이 아니라 `npm run check:surface` 가 build-time 에 기계 강제한다 — 아래 규약 7.**

**relevance taxonomy** (분류 기준):

| class | 처리 | 예 |
|---|---|---|
| **user-fund-move** | COVER | supply/withdraw/borrow/repay/swap/transfer |
| **permission-grant** ⚠️ | **항상 COVER** (아래 red-flag) | approve/permit/setApprovalForAll/setAuthorization/delegate |
| keeper·integrator | EXCLUDE: 비-user-wallet | liquidate/flashLoan/keeper-only |
| governance (onlyOwner) | EXCLUDE: DAO/admin | setFee/enableX/setOwner |
| infra/callback | EXCLUDE: 비-pre-sign | accrueInterest/onERC721Received/internal |

```jsonc
// research-<protocol>.json (스크래치, 작업용)
{
  "protocol": "<name>", "category": "dex|lending|liquid-staking|restaking|perp|...",
  "contracts": [{
    "name": "Pool",
    "addresses": { "1": "0x...", "8453": "0x..." },   // 체인별 정확 주소 (cartesian 금지)
    "abi_source": "sourcify|etherscan-verified|github", "abi_url": "https://...",
    "external_functions": [   // ★ 전수 — 하나도 빠뜨리지 않는다. 각 함수에 triage.
      { "selector": "0x...", "sig": "supply(...)",               "triage": "COVER",   "class": "user-fund-move" },
      { "selector": "0x...", "sig": "setAuthorization(address,bool)", "triage": "COVER",   "class": "permission-grant" },
      { "selector": "0x...", "sig": "setFee(...)",               "triage": "EXCLUDE", "class": "governance", "reason": "onlyOwner" }
    ]
  }]
}
```

### 규약
1. **주소/ABI 는 1차 출처만** — 공식 docs / Sourcify / Etherscan·BaseScan verified `Read/Write Contract` 탭 / 공식 GitHub. 블로그·AI·wiki 금지.
2. **selector cross-check** — `selector == keccak256(signature)[0:4]`. (검증: `cast sig "transfer(address,uint256)"` == `0xa9059cbb`. cast 없으면 alloy/로컬 keccak.)
3. **전수 후 triage** — hub 의 external state-changing 함수를 *전부* 나열한 뒤 각각 `COVER`/`EXCLUDE:reason`. "눈에 띄는 것만" 금지. EXCLUDE 는 사유 명시(`onlyOwner`/keeper/callback/internal). COVER 는 EOA·smart-account 가 서명 직전 직접 부르는 fund-move·permission 함수.
4. **⚠️ permission-primitive red-flag (반드시 통과)** — `authorize | approve | permit | delegate | setOperator | setApprovalForAll | setAuthorization` 패턴 함수는 **무조건 COVER** (Tier 3 escalate 필요해도). ScopeBall 의 존재 이유(권한 위임 분석)라서 EXCLUDE/Unknown/skip **금지**. on-chain calldata 와 off-chain EIP-712(`*WithSig`/typed-data) **양쪽** 점검 — 둘 다 권한 grant 다. 매핑할 ActionBody 가 없으면 §4a 에서 Tier 3 신규 action 추가(Unknown 으로 떨구지 않음).
5. **체인 scope** — main chain 1개 + L2 variant. free Etherscan v2 키는 **Base(8453)/Optimism(10) 미지원**(아래 5b) — 그 체인은 Dune 또는 유료키.
6. **legacy 명시 제외 기록** — 구버전 deploy 제외 결정을 적어둠.
7. **⚠️ executable gate — 함수 차원 (I1~I3, 산문 → build 강제)** — 위 전수 triage 는 agent 재량에 맡기면 silent 누락이 재발한다(§9 setAuthorization). 그래서 **기계 검증**으로 승격: scratch `research-<protocol>.json` 을 commit 되는 2 artifact 로 — `registryV2/surface/<protocol>/<contract>.abi.json`(1차 출처 verified **전체 ABI** snapshot = manifest·triage 가 거짓말 못 하는 **독립 ground-truth**) + `.coverage.json`(per-selector `COVER`/`EXCLUDE:reason` + `signed_structs`). `npm run check:surface` 가 검사: **I1** snapshot 의 external-mutating selector(`stateMutability∈{nonpayable,payable}`) 전수가 coverage 에 있나(누락 = 원래 miss) · **I2** COVER 는 manifest 보유 · **I3** manifest 는 COVER · **S1/S2** typed-data ↔ `signed_structs`. 위반 = exit 1. coverage·manifest 를 **둘 다** 빠뜨려도 독립 snapshot 이 I1 으로 잡으므로 silent 누락이 *불가능*해진다. 절차·포맷 = `registryV2/surface/README.md`. (이게 §3 의 본질: research-completeness 를 trust 에서 build-enforced invariant 로 — Cedar 등록 누락을 `MissingAction` 이 잡는 것과 같은 패턴.)

8. **⚠️ executable gate — 컨트랙트 차원 (I0, 규약 7 의 전제)** — **규약 7(I1)은 "찾은 컨트랙트 *안*의 함수 완비"만 강제한다. 리서치가 통째로 놓친 *컨트랙트*는 못 잡는다** — snapshot 을 안 떴으니 I1 이 돌 대상이 없고, P2 의 실거래 pull 도 **`txlist&address=` 라 그 주소를 query 조차 안 한다**(주소가 리서치 산출물이라, 못 찾은 주소는 테스트도 장님). 즉 **contract-inventory 는 리서치의 single point of failure** 였다. 이를 같은 패턴으로 한 층 위에서 강제: `registryV2/surface/<protocol>/_deployments.json` = **공식 1차 deployment 목록**(= 컨트랙트 인벤토리의 독립 ground-truth, 함수에 대한 verified ABI 와 동형) — 모든 deployed 컨트랙트를 `cover`(snapshot 보유 강제) 또는 `exclude:reason` 로 전수 triage. `check:surface` 의 **I0**: 모든 `cover` 가 surface snapshot 을 갖나(없으면 = user-facing 컨트랙트 누락 → exit 1) · `exclude` 는 reason 보유 · I0′ gated 인데 목록에 없으면 WARN. `_deployments.json` 없는 프로토콜은 **"contract-inventory NOT enforced" WARN**(opt-in, 비파괴). **정직한 floor**: I1 의 ABI 는 함수를 못 빠뜨리지만(존재하면 ABI 에 있음), deployment 페이지는 컨트랙트를 *누락할 수 있다* → I0 는 "공식 목록만큼" 완벽(I1 보다 약함). SPOF 를 "agent 기억"에서 "공식 목록 대비 build diff + 아래 aggregator cross-check"로 옮길 뿐 airtight 는 아니다.

   **`_deployments.json` ground-truth 소스 (1차 우선):**
   | tier | 소스 | 비고 |
   |---|---|---|
   | **1차** | 프로토콜 공식 docs "Deployments/Addresses" 페이지 · 공식 GitHub deploy artifact(`@aave/address-book`, Uniswap deployments JSON, hardhat-deploy `deployments/`, foundry `broadcast/`) · 온체인 registry(Curve `AddressProvider`, Aave `PoolAddressesProvider`) | **I0 목록의 정본.** Lido = `docs.lido.fi/deployed-contracts`(§9.10 실측) |
   | **2차 (discovery cross-check)** | **DefiLlama-Adapters** GitHub(프로토콜별 컨트랙트 주소 나열 — 강력) · **Dune** `<project>` decoded 네임스페이스/contract registry · **Etherscan/Basescan** address labels·Label Cloud · **Sourcify** verified repo | 1차가 누락한 컨트랙트를 *도전*하는 sweep. 반드시 1차로 재검증 후 `_deployments.json` 등재 |
   > **단일 완전+권위 레지스트리는 없다**(정직). best = 공식 deploy artifact(1차)를 `_deployments.json` ground-truth 로 + DefiLlama/Dune/Etherscan-labels **+ LLM discovery panel(§3.1)** 을 누락-도전 sweep 으로 교차.

### 3.1 P0 dual-agent research panel — Codex + Claude Code

P0 contract discovery 는 **다운스트림 안전망이 없는 유일한 완비성 층**이다(I0 floor = "그 컨트랙트가 있는 줄 몰랐다"; I1·실거래 pull 둘 다 리서치가 찾은 주소에 갇힘). 그래서 새/복잡/멀티컨트랙트 프로토콜은 **현재 Codex 세션과 Claude Code 에 같은 리서치를 병렬 수행**시킨 뒤, 현 세션이 두 결과를 통합·검증한다. 이미 온보딩된 프로토콜도 동일하다: "이미 있음"을 믿지 말고 두 리서치 결과와 현재 `surface/`·manifest 를 diff 한다.

> **★ 비협상 규율 — LLM = candidate 생성기(untrusted), 1차 fetch + I0/I1 gate = disposer(trusted).** LLM 은 주소·ABI 를 **환각**한다; 모델 N개가 *같은 틀린 주소*에 합의해도 틀린 것이다. **LLM 합의를 ground-truth 로 쓰지 말 것** — 후보만 넓히고, 통과는 항상 1차 출처 fetch + gate 가 시킨다.

**절차 (fan-out → synthesize → verify):**
1. **Codex 현 세션 리서치** — 공식 docs/GitHub/Sourcify/Etherscan/Dune namespace 를 읽고 candidate contract/function inventory 작성.
2. **Claude Code 병렬 리서치** — 같은 프롬프트를 Claude Code headless 로 실행. 예:
   ```bash
   claude -p "<P0 discovery prompt>" --add-dir /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2
   ```
   Claude 결과는 바로 신뢰하지 않고 `/tmp/p0-<protocol>-claude.md` 같은 scratch 로만 보관한다(커밋 금지).
3. **동일 discovery 프롬프트** — 두 agent 모두 아래 정보만 요구한다:
   > `Protocol: <name>. <chains> 에 배포된, 일반 user(EOA/smart account)가 서명 직전 직접 호출/EIP-712 서명하는 **모든** 컨트랙트를 나열하라. 각: name · chain · address · **1차 출처 URL**(공식 docs "deployments" / 공식 GitHub deploy artifact). 각 컨트랙트의 external state-changing 함수도. 공식 출처만 인용 — 주소 추측 금지, 불확실하면 "unverified" 표기. 표로 출력.`
4. **synthesize (현 세션)** — Codex 결과 ∪ Claude 결과 ∪ 공식 deploy list ∪ DefiLlama/Dune/Etherscan-label sweep 을 dedup. 한쪽 agent 만 언급한 컨트랙트/함수는 **검증 우선 후보**로 표시한다.
5. **verify (gate 가 dispose)** — 각 후보 컨트랙트를 **1차 fetch**(공식 deploy page/GitHub artifact/verified ABI) → `surface/<protocol>/_deployments.json` 에 cover/exclude:reason 등재 → cover 는 `.abi.json` snapshot + `.coverage.json` triage 작성 → `npm run check:surface`(I0/I1). **1차 verify 안 되는 후보는 drop**.
6. **adversarial review** — 통합 인벤토리를 Claude 또는 Codex 에 다시 던져 `"이 목록에서 빠진 user-facing 컨트랙트/함수는? 공식 출처로."` 를 묻는다. 새 후보가 나오면 5 로 되돌린다.
7. **P0 evidence ledger** — `crates/integration-tests/onboarding/<protocol>/evidence.md` 에 source list, Claude/sub-agent exact command or agent id, Codex-only 후보, Claude-only 후보, dropped-unverified 후보, final cover/exclude count, `check:surface` 출력 요약을 기록한다. 로컬 scratch 로그만 남기고 이 ledger 를 안 채우면 P0 는 미완이다.

**portability 원칙:** core P0(1차 fetch + I0/I1 gate)는 외부 LLM 없이도 단독 완결돼야 한다. Claude Code panel 은 recall 을 높이는 렌즈이지 trust anchor 가 아니다. 어떤 소스(LLM 포함)도 모르는 컨트랙트는 여전히 invisible 이므로, 최종 완비성 floor 는 `_deployments.json` 의 1차 출처 + gate 범위만큼이다.

### 3.2 P0 token-surface inventory — ERC 표준 callkey 의 입력

프로토콜 온보딩은 컨트랙트/selector 만으로 끝나지 않는다. `registryV2/tokens/<chainId>/<addr>.json` 은 ERC 표준 manifest(`tokens:erc20`/`tokens:erc721`/`tokens:erc1155`)의 **build-time enumerate 입력**이다. 즉 프로토콜이 만든 LP/share/receipt/debt/governance/base token 이 누락되면 `approve`/`transfer`/`transferFrom`/`permit` 같은 표준 호출이 해당 주소에서 callkey 를 못 만들어 `no_declarative_v3_mapper` 로 빠진다. LayerZero ZRO 보강이 이 실패 모드의 실측 사례다.

**필수 절차:**
1. `crates/integration-tests/TOKEN_INVENTORY_GUIDE.md` 를 읽고 `(chainId, protocol)` 단위로 token-surface 를 작성한다.
2. 공식 docs/address-book/pool list/explorer token page 같은 **정적 1차 출처**로 token 주소, symbol, decimals, name, semantic `token_kind` 를 확인한다. on-chain RPC enumerate 로 symbol/decimals 를 임의 조회하지 않는다.
3. `registryV2/tokens/<chainId>/<lowercase-addr>.json` 을 작성한다. underlying/peg/pool ref 가 가리키는 토큰이 없으면 그 underlying 도 함께 등록한다.
4. large pool protocol(Curve/Aerodrome/Balancer 등)은 canonical/major market 우선 batch 를 허용하되, **어떤 pool/token 을 이번 batch 에 포함/제외했는지 P0 로그에 명시**한다. silent long-tail 누락 금지.
5. `cd registryV2 && npm run build` 또는 `npx tsx scripts/build-index.ts` 로 token JSON shape + auto-enumerate 를 검증한다.
6. P2 real-tx 에서 token contract 직접 호출(`to=<token address>`)이 `no_declarative_v3_mapper` 로 나오면 먼저 token registry 누락인지 확인한다.

**Curve 예시:** Curve 는 단일 router/factory 뿐 아니라 pool LP token 이 곧 ERC20 share token 인 경우가 많다. 따라서 Curve onboarding 은 CRV/crvUSD 같은 governance/base token, 각 covered pool 의 LP token(`lp_share{pooled,fungible}`), pool underlyings, gauge/stake receipt 토큰(스코프에 포함될 때)을 함께 조사해야 한다. Compound 의 `cToken` 같은 lending receipt 와 동일하게, Curve LP token 도 “프로토콜이 만들어낸 user-held receipt/share” 이므로 토큰 레지스트리 대상이다.

---

## 4. P1 — AUTHOR

함수마다 **3a schema → 4b manifest → 4c engine** 순. schema 가 manifest 보다 먼저인 이유: 존재하지 않는 domain/action 을 target 하는 manifest 는 테스트에서 hard-fail 하기 때문.

### 4a. Tier 3 — action-schema 매핑

각 함수가 어느 `ActionBody` variant 로 가는지 결정한다.

#### Decision tree
```
함수 X 의 의미가:
  기존 domain + 기존 action 에 매핑됨?   → YES: 스키마 무수정, 4b 로
  기존 domain, 새 action?                → 그 domain 모듈에 variant + 파일 추가 (release PR)
  새 domain 필요?                        → ActionBody variant + 모듈 + VALID_DOMAINS + Cedar + TS (큰 blast)
  ⚠️ 권한 grant(approve/authorize/delegate…)인데 맞는 action 없음? → Tier 3 신규 action 필수 (Unknown 금지 — §3 red-flag). market 무관 grant 면 venue 없는 bespoke locator 가능 (Morpho SetAuthorization 선례).
  매핑 불가 + 권한 grant 아님 (admin/opaque)? → Unknown 허용 (warn/deny, 테스트는 excluded)
```

#### Tier3 downstream contract — ActionBody 만 추가하면 끝이 아니다

Tier3 확장은 세 레이어 계약이다:

```text
manifest/Tier2 builder
  → ActionBody                         crates/simulation/reducer/src/action/**
  → lowering_v2::lower_action          crates/policy-engine/src/lowering_v2/**
  → Cedar context                      schema/policy-schema/actions/**/*.cedarschema
```

필수 산출물:
1. **ActionBody schema** — `crates/simulation/reducer/src/action/<domain>/**`: protocol-specific 이름이 아니라 user intent 를 표현하는 domain/action/field.
2. **effect/reducer/view/sync touchpoint** — `crates/simulation/reducer/src/effect/**`, `action/view.rs`, 필요 시 `simulation/sync/src/action_walk/**`.
3. **lowering_v2** — `crates/policy-engine/src/lowering_v2/<domain>/<action>.rs` + `<domain>/mod.rs`: Rust `snake_case` 를 Cedar `camelCase` context 로 손매핑. `U256`/`U128` 는 lower-hex string, `Address` 는 lowercase hex, `LiveField<T>` 는 `.value` 만 노출, optional 은 absent 로 omit.
4. **cedarschema** — `schema/policy-schema/actions/<domain>/<action>.cedarschema`: `<Action>Context`, `<Action>CustomContext = {};`, `action "<PascalAction>" appliesTo { principal: Wallet, resource: Protocol, context: <Action>Context }`.
5. **schema registration** — `crates/policy-engine/src/schema/mod.rs`, `action_name.rs`, `per_policy.rs` 의 resolver table. auto-discovery 아님.
6. **conformance test** — leaf lowering test 에서 `test_support::assert_conforms(...)` 로 실제 `compose_per_policy` schema strict validation. ActionBody ↔ lowering ↔ cedarschema drift 를 여기서 잡는다.
7. **manifest/corpus** — 새 action tag 를 target 하는 manifest + `check:manifest` + field-level golden/`expect_body`.

예: `AmmAction::Swap(SwapAction)` 은 `action/amm/swap.rs` 가 Rust schema, `lowering_v2/amm/swap.rs` 가 `Amm::Action::"Swap"` + `Amm::SwapContext` JSON, `schema/policy-schema/actions/amm/swap.cedarschema` 가 policy-visible context 를 정의한다. 셋 중 하나라도 필드명/타입/필수성/action uid 가 어긋나면 policy validation 이 실패하거나 정책 작성자가 필요한 필드를 못 본다.

#### ActionBody 카탈로그 (현재 — `simulation/reducer/src/action/`, serde `tag="domain"`)

최상위 variant (`action/mod.rs` — **작성 전 `grep -n "pub enum ActionBody"` 로 카운트 재확인**, 늘어난다):
`Token` · `Amm` · `Lending` · `Airdrop` · `Launchpad` · `Perp` · `LiquidStaking` · `Permission` · `Yield` · `Restaking` · `Staking` · `HyperliquidCore` · `Multicall { actions: Vec<ActionBody> }` · `Unknown { target, chain, calldata, value }`

각 domain 의 action (`tag="action"`, snake_case). **작성 전 해당 `<domain>/mod.rs` 를 직접 읽어 현재 variant/필드 재확인**(스키마는 늘 확장된다):

| domain | 파일 | action variant |
|---|---|---|
| **token** | `token/mod.rs` | erc20_approve, erc20_permit, permit2_approve, permit2_sign_allowance, erc20_transfer, nft_approve, nft_set_approval_for_all, nft_transfer, revoke_approval |
| **amm** | `amm/mod.rs` | swap, add_liquidity, remove_liquidity, collect_fees, sign_intent_order, cancel_intent_order |
| **lending** | `lending/mod.rs` | supply, withdraw, borrow, repay, swap_rate_mode, set_e_mode, enable_collateral, disable_collateral, delegate_borrow, liquidate |
| **airdrop** | `airdrop/mod.rs` | claim, delegate |
| **launchpad** | `launchpad/mod.rs` | commit, claim_allocation, claim_vested, refund, withdraw_commit |
| **perp** | `perp/mod.rs` | open_position, close_position, increase_position, decrease_position, adjust_margin, change_leverage, change_margin_mode, place_limit_order, place_stop_order, cancel_order, claim_funding |
| **liquid_staking** | `liquid_staking/mod.rs` | stake, wrap, unwrap, request_withdrawal, claim_withdrawal, transfer_shares |
| **permission** | `permission/mod.rs` | protocol_authorization (operator/relayer 권한 위임 — Compound `allow`/Balancer relayer 등 protocol-specific grant) |
| **yield** | `yield_/mod.rs` | pt_swap, yt_swap, add_market_liquidity, remove_market_liquidity, mint_py, redeem_py, mint_sy, redeem_sy, claim_yield, sign_limit_order, cancel_limit_order |
| **restaking** | `restaking/mod.rs` | delegate_to, redelegate, undelegate, deposit, queue_withdrawal, complete_withdrawal, register_operator |
| **staking** | `staking/mod.rs` | lock, increase_lock_amount, increase_lock_time, unlock, claim_rewards, vote_for_gauge, gauge_deposit, gauge_withdraw |
| **hyperliquid_core** | `hyperliquid_core/mod.rs` | hl_order, hl_update_leverage, hl_withdraw, hl_usd_send, hl_approve_agent |

각 action 의 필드 예 (`token/erc20_approve.rs`):
```rust
pub struct Erc20ApproveAction { pub token: TokenRef, pub spender: Address, pub amount: U256 }
```
`venue` 가 있는 domain(amm/lending/perp)은 별도 venue enum 으로 프로토콜 식별:
- `AmmVenue`: UniswapV2/V3/V4, SushiV2, CurveV1/V2, BalancerV2/V3, TraderJoeLB, MaverickV2, AggregatorRoute …
- `LendingVenue`: AaveV3/V2, CompoundV3/V2, MorphoBlue, MorphoOptimizer, Spark, Fluid …
- `PerpVenue`: Hyperliquid, GmxV2, DyDxV4, Vertex, Aevo, Drift, JupiterPerps, Synthetix, Generic …
- `StakingVenue`: Lido (liquid_staking) …

> 새 프로토콜이 기존 domain 에 맞지만 venue 가 없으면 → **venue enum 에 variant 추가**(Tier 3, 가벼운 편). 예: 새 lending 프로토콜 → `LendingVenue` 에 추가.

#### 새 action 추가 절차 (기존 domain)
예: lending 에 `flash_loan` 추가.
1. `crates/simulation/reducer/src/action/lending/flash_loan.rs` 새 파일 — action struct (`#[derive(Serialize, Deserialize, Tsify, ...)]`).
2. `lending/mod.rs`: `pub mod flash_loan;` + `pub use self::flash_loan::*;` + enum 에 `FlashLoan(FlashLoanAction)` + `action_tag()` arm `Self::FlashLoan(_) => "flash_loan"`.
3. manifest 작성(4b) — `body.lending.flash_loan`.
4. `action_builder.rs` 의 live_inputs default 카탈로그에 (domain,action,field) 추가(해당 함수가 live_inputs 참조 시).
5. Rust smoke test (mod.rs `#[cfg(test)]`) + corpus 실거래 1건.

#### 새 domain 추가 (드묾, 큰 blast)
위 + 반드시 **동기화**:
- `crates/integration-tests/src/harness/oracle.rs:22` `VALID_DOMAINS` 에 새 domain 문자열 추가 (안 하면 하니스가 invalid domain 으로 fail).
- `action_builder.rs::flatten_body` 의 cross-cutting 분기(multicall/unknown 류 특수 처리) 검토.
- `crates/policy-engine/src/lowering_v2/dispatch.rs` + `lowering_v2/<domain>/` + `schema/policy-schema/actions/<domain>/*.cedarschema` + schema resolver 등록 + conformance test.
- TS(tsify) 바인딩은 `wasm-build` 에서 재생성. 손수 `.d.ts` 를 수정하지 않는다.

#### Unknown fallback
매핑 불가/희귀 함수는 manifest 를 안 만들거나 `body.domain="unknown"` 로 둔다 → `ActionBody::Unknown{target,chain,calldata,value}`. 테스트는 `excluded`(아래 5e). **디코드를 막지 않는다.**
> ⚠️ Unknown 은 *진짜 opaque/admin* 호출용이지 **알려진 권한 grant 용이 아니다.** 권한 위임을 Unknown 으로 두면 분석기가 "전권 위임"을 opaque calldata 로만 보여줘 **과소경고** — ScopeBall 의 핵심 실패. 권한 grant 는 가치가 항상 높아 extension guide rule 3 의 'value<cost→Unknown' 을 override → Tier 3 escalate (§3 red-flag).

### 4b. Tier 1 — manifest 작성

#### 최상위 구조 (schema v3)
```jsonc
{
  "type": "adapter_action",          // v3 (v1 은 "adapter_function" — 안 씀)
  "id": "<publisher>/<contract>/<func>@1.0.0",
  "schema_version": "3",             // 필수
  "match": {
    "selector": "0x<8hex>",
    // (A) 정확 주소 맵:
    "chain_to_addresses": { "1": ["0x..."], "8453": ["0x..."] }
    // (B) 또는 ERC 표준 자동 enumerate:
    // "chain_to_addresses_source": "tokens:erc20", "chain_ids": [1,10,8453,42161]
  },
  "abi_fragment": { "function_name": "...", "abi": { "name","type":"function","inputs":[...] } },
  "emit": { "strategy": "...", /* strategy별 필드 */ },
  "requires": { "imperative": [], "adapter_capabilities": [], "host_capabilities": [], "extension": ">=0.1.0" }
}
```
디렉토리: `registryV2/manifests/<publisher>/<contract>/<func>@<semver>.json` (publisher = ENS/slug, 표준은 `standard`).

#### Strategy 선택 (decision tree)
```
calldata → ActionBody 가:
  단순 평면 1:1 ?                              → single_emit
  bytes[] self-recursion (각 원소 = 같은 컨트랙트 다른 함수)?  → multicall_recurse
  command byte stream (bytes commands, bytes[] inputs) — UR류? → opcode_stream_dispatch
  동질 tuple-array → N개 ActionBody (Permit2 batch류)?         → array_emit
  enum-tagged userData (CoreWriter류)?         → tagged_dispatch
```

#### ① single_emit — 실제 예 (`standard/erc20/approve@1.0.0.json`, 전문)
```json
{
  "type": "adapter_action",
  "id": "standard/erc20/approve@1.0.0",
  "schema_version": "3",
  "match": { "selector": "0x095ea7b3", "chain_to_addresses_source": "tokens:erc20", "chain_ids": [1, 10, 8453, 42161] },
  "abi_fragment": {
    "function_name": "approve",
    "abi": { "name": "approve", "type": "function", "stateMutability": "nonpayable",
      "inputs": [ { "name": "spender", "type": "address" }, { "name": "amount", "type": "uint256" } ],
      "outputs": [ { "name": "", "type": "bool" } ] }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "token",
      "token": {
        "action": "erc20_approve",
        "erc20_approve": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$to" } },
          "spender": "$args.spender",
          "amount": "$args.amount"
        }
      }
    },
    "live_inputs": {}
  },
  "requires": { "imperative": [], "adapter_capabilities": ["token_metadata"], "host_capabilities": [], "extension": ">=0.1.0" }
}
```
구조: `emit.body` = 중첩 `{domain, <domain>: {action, <action>: {...필드}}}`.

#### ② opcode_stream_dispatch — 실제 예 (`uniswap/universal-router/execute-v1@1.0.0.json`, emit 발췌)
```jsonc
"emit": {
  "strategy": "opcode_stream_dispatch",
  "dispatcher_id": "universal_router",
  "mask": "0x7f",                  // command byte & mask = opcode
  "allow_revert_bit": "0x80",      // 최상위 비트 = allow-revert (무시)
  "unknown_opcode_policy": "warn", // deny | skip | warn
  "per_opcode_body": {             // 키 = opcode hex, 값 = {name, inputs_abi, body}
    "0x00": {
      "name": "V3_SWAP_EXACT_IN",
      "inputs_abi": "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)",
      "body": { "domain": "amm", "amm": { "action": "swap", "swap": {
        "venue": { "name": "uniswap_v3", "chain": "$chain", "pool": "$resolved.pool", "fee_tier_bp": "$resolved.fee_tier_bp" },
        "params": {
          "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.v3_path_first_token" } },
          "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.v3_path_last_token" } },
          "direction": { "kind": "exact_input", "amount_in": "$inputs.amountIn", "min_amount_out": "$inputs.amountOutMin" },
          "recipient": "$inputs.recipient"
        } } } }
    },
    "0x0a": { "name": "PERMIT2_PERMIT", "inputs_abi": "(...)", "body": { /* ... */ } }
  }
}
```
핵심: opcode 별 `body` 안에서 그 opcode 의 디코드된 인자는 `$inputs.<name>` 으로 참조. opcode payload 의 ABI 는 `inputs_abi` 로 manifest 가 선언(V3 는 Rust 테이블이 아니라 manifest 가 opcode 타입을 들고 있다).

#### ③ multicall_recurse — 실제 예 (`uniswap/v3-nfpm/multicall@1.0.0.json`, 전문)
```json
{
  "type": "adapter_action", "id": "uniswap/v3-nfpm/multicall@1.0.0", "schema_version": "3",
  "match": { "selector": "0xac9650d8", "chain_to_addresses": {
    "1": ["0xC36442b4a4522E871399CD717aBDD847Ab11FE88"], "8453": ["0x03a520b32C04BF3bEEf7BEb72E919cf822Ed34f1"] } },
  "abi_fragment": { "function_name": "multicall",
    "abi": { "name": "multicall", "type": "function", "inputs": [ { "name": "data", "type": "bytes[]" } ] } },
  "emit": { "strategy": "multicall_recurse", "recurse_rule_id": "self_array_bytes_last_arg", "max_depth": 3 }
}
```
핵심: `body` 없음. 엔진이 `data: bytes[]` 의 각 원소를 **같은 컨트랙트의 다른 함수 호출**로 보고 재귀 분해 → `ActionBody::Multicall{actions}`. inner selector 도 indexed/manifest 가 있어야 완전 분해.

#### ④ array_emit — 실제 예 (`uniswap/permit2/permitBatch@1.0.0.json`, emit 발췌)
```jsonc
"emit": {
  "strategy": "array_emit",
  "array_source": "$args.permitBatch[0]",   // 펼칠 배열 경로
  "body": {                                  // 배열 원소 1개당 ActionBody (원소는 $inputs[...])
    "domain": "token", "token": { "action": "permit2_sign_allowance", "permit2_sign_allowance": {
      "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs[0]" } },
      "spender": "$args.permitBatch[1]", "amount": "$inputs[1]", "expires_at": "$inputs[2]", "sig_deadline": "$args.permitBatch[2]"
    } } },
  "live_inputs": { /* onchain_view ... */ }
}
```
→ `ActionBody::Multicall{actions}` (원소별 1 action). **V3 형태는 `array_source` + `body`** (V1 의 `array_path`/`fields` 아님).

#### emit.body placeholder 문법 (현재)
| placeholder | 의미 | 비고 |
|---|---|---|
| `$chain` | chain id | root-only (suffix 불가) |
| `$to` | tx target 주소 | root-only |
| `$calldata` | raw calldata hex | root-only (Unknown 보존용) |
| `$tx.value` `$tx.from` `$tx.to` `$tx.chain` | tx 메타 | |
| `$args.<name>` | 디코드된 함수 인자 | |
| `$args.x[0][1]` | **chained-numeric** nested tuple/array | 음수 = tail(`[-1]`) |
| `$inputs.<name>` / `$inputs[0]` | opcode/array item 의 인자 | opcode_stream/array_emit 안에서 |
| `$resolved.<k>` | 정적 lookup (weth, pool_manager, pool, fee_tier_bp) | host/엔진 채움, 미해소 시 zero |
| `$derived.<k>` | 절차적 계산 (v3_path_first_token 등) | |

**⚠️ dotted `$args.x.y` 는 미지원.** nested 접근은 반드시 **chained-numeric** `$args.x[i][j]`. (예: Permit2 `permitSingle.details.token` → `$args.permitSingle[0][0]`. 이건 commit `3f93f5c` 에서 디코더의 nested-tuple uint coercion 과 함께 동작 확정됨 — 과거엔 깨졌으나 현재 정상.)

AssetRef 표준: `{ "key": { "standard": "erc20|erc721|erc1155", "chain": "$chain", "address": "<addr placeholder>" } }`.
`live_inputs` 항목: `{ "<field>": { "source": { "kind": "onchain_view|derived_from|oracle_feed|venue_api", ... }, "ttl_s": N } }`. 테스트에선 default 스텁이라 값 미채움 — 단 **manifest 에 선언은 필요**(스키마 필드 충족).

### 4c. Tier 2 — generic 엔진 확장 (V3 Tier B)

대부분 함수는 4b manifest 로 끝난다. **generic 엔진이 표현 못 하는 shape 만** 여기 해당:

| 깨지는 shape | 증상 | 확장 위치 |
|---|---|---|
| nested tuple per-component 타입 유실 (uint48 등 string 화) | `build_action_body_failed: invalid type string, expected u64` | `action_builder.rs` 의 placeholder 해소/coercion (walk_json_path + decoded_value_to_json_typed). **Permit2 류는 3f93f5c 에서 이미 해결** — 새 프로토콜에서 재발 시 같은 패턴으로 |
| opcode sub-stream nested 전개 (UR V4 action-stream 등) | `build_multicall_failed` (pool_id/currencyIn 등) | `action_builder.rs::build_multicall_from_opcode_stream` + dispatch (현재 deferred baseline = commit `70c34a7`) |
| 동적 길이 배열 cross-index (`assets[swaps[i].assetInIndex]`, Balancer batchSwap) | declarative 로 cross-array join 불가 | array_emit 으로 표현 불가 → 엔진/별도 처리 |
| packed bytes32 bit-slice (Aave L2Pool) | reserve index + bit-field, asset 가 주소 아님 | bit-slice 디코드 + onchain reserve→address (live_inputs) |

**확장 절차(TDD):** corpus 에 `expect:error` baseline → `action_builder.rs`(또는 `declarative_exports.rs` dispatch arm) 수정 → corpus `expect:pass` flip → `cargo test --workspace` green → WASM 재빌드. 착지 불가하면 corpus `_note` 로 문서화 + defer.

> Tier 2 변경은 WASM 에 들어가는 Rust → release PR. WASM 사이즈 예산 점검(memory: 6 MiB target).

### 4d. ENRICHMENT — action 별 live_field 적정성 (★ 디코드 ≠ 완료)

ScopeBall 은 사용자가 **서명 직전 intent 를 이해**하게 하는 도구다. 그러므로 P1 의 마지막 질문은 "디코드가 됐나"가 아니라 **"디코드된 필드가 사용자에게 그 자체로 읽히나? 안 읽히면 무엇으로 환산해 보여줘야 하나?"** 다. 이 단계를 건너뛰면 모든 게이트(check:surface / check:manifest / corpus / workspace)가 green 인데도 **user-illegible** 한 어댑터가 착지한다 — Lido 1차 온보딩이 정확히 그랬다(§9.9). decode-faithful("raw 필드만, live_inputs 비움")은 set_authorization 같은 **이미 읽히는** action 에만 맞다; **모든** action 의 기본값이 아니다.

> 이건 §3 surface-completeness(가로 = 함수 다 덮었나)의 **세로 짝(깊이 = 덮은 action 의 intent 가 읽히나)** 이다. 단 surface 는 독립 ABI snapshot 으로 build 강제가 되지만, **enrichment 는 "이 필드가 사용자에게 읽히나"라는 제품 판단이라 객관 오라클이 없어 build-gate 불가** → 아래 decision-tree + §8.6 self-check + golden 으로 prescribe 한다(정직한 한계).

#### Enrichment decision-tree (COVER action 마다, 필드별)
```
이 필드가 사용자에게 그 자체로 의미가 읽히나?
  ├ YES — token-unit amount / address / bool / 이미 사람이 읽는 값   → live_field 불필요 (raw 그대로)
  └ NO  — 추상·불투명·간접 단위 (domain 무관):
       · 프로토콜 내부 share/unit (Lido shares, Morpho share, vault share)
       · wrapped/rebasing 수량 (wstETH↔stETH, cToken↔underlying)
       · 내부 index / id (coin index, reserve index, market id)
       · rate-dependent (LST exchange rate, APY, health factor, perp 펀딩·청산가·leverage)
       · 간접 참조 (airdrop merkle eligibility, restaking operator/AVS 위임량, NFT tokenId→metadata)
            → 그 필드를 사용자 단위/사실로 바꾸는 live_field 를 action 에 추가
            → 추가 못 하면(소스 모호/배열·tuple 모델링 과중) corpus _note 또는 plan 에 **사유 명시 defer**
```
판단 기준 = "이 값만 보고 사용자가 '내가 무엇을 얼마나 commit 하나'를 아는가?". 모르면 enrich. (환산이 항상 수량(U256)인 건 아니다 — bool eligibility, Decimal rate, struct state 도 live_field 다. 타입별 = 아래 타입표.)

#### ★ 결정적 제약 — live_field source 가 calldata 인자를 쓰면 manifest-only 가 아니다
`DataSource::OnchainView { chain, contract, function, decoder_id }` 에는 **args 필드가 없다**(`simulation/state/src/live_field/source.rs`). 즉 `getPooledEthByShares(shares)` 처럼 **디코드된 calldata 값을 view 인자로 넘겨야 하는** live_field 는 manifest 의 `onchain_view` 만으로 불가능하다. 인자는 sync 시점에 **`crates/simulation/sync/src/args_resolver.rs` 의 `resolve_args(slot, action, state)`** 가 `ActionSlot` 별로 action 에서 추출해 인코딩한다 → **Tier B Rust 필수**. (인자 없는 view(`getTotalShares()`)나 derived 계산은 manifest-only 가능.)

| live_field 종류 | manifest-only? | 작업 |
|---|---|---|
| 인자 없는 view (`getTotalShares()`) / oracle_feed / derived_from | ✅ | manifest `live_inputs.source` 만 |
| **calldata 인자 필요한 view** (`getPooledEthByShares(shares)`) | ❌ | + Tier B `ActionSlot` + `args_resolver` arm |

#### 5-touchpoint 레시피 (lending::supply 전 경로 미러 — `SupplyLiveInputs` 가 정본)
calldata-인자 환산 live_field 를 한 action 에 추가하는 전 경로. (인자 없으면 B 생략.)

| | touchpoint | 위치 (symbol — `grep` 재확인) |
|---|---|---|
| **A** reducer | `<Action>LiveInputs { <field>: LiveField<T> }` struct + action 에 `pub live_inputs` 필드 (non-optional, manifest 가 항상 emit) | `reducer/src/action/<domain>/<action>.rs` |
| **B** sync (Tier B) | ① `ActionSlot` variant ② `action_walk/<domain>.rs` 의 `walk`(`push_if_stale(&li.<field>, slot)`) + `apply`(slot match→`set_field`) + mod dispatch arm ③ `args_resolver::resolve_args` arm — action 에서 인자 추출 후 `encode_u256`(U256) / `encode_address`(addr) (둘 다 `fetchers/decoder.rs` 에 **이미 존재**) | `sync/src/walker.rs` · `sync/src/action_walk/<domain>.rs` · `sync/src/args_resolver.rs` |
| **C** Cedar+lowering | cedarschema 에 host-populated field(non-optional, LiveField→inner T flatten; **타입별 = 아래 타입표**) + `lowering_v2/<domain>/<action>.rs` 가 `.value` 를 emit(타입별) + `test_support` skeleton 헬퍼 + conform 테스트 생성자에 live_inputs 추가 | `schema/policy-schema/actions/<domain>/<action>.cedarschema` · `lowering_v2/<domain>/{<action>,mod}.rs` |
| **D** generic 엔진 | `live_input_default` 카탈로그에 `(domain, action, field) => skeleton`(**타입별 = 아래 타입표**). layout 은 default `Nested` 라 보통 변경 0 | `mappers/src/declarative/action_builder.rs` |
| **E** manifest | `emit.live_inputs.<field>.source`(Morpho supply `reserve_state` shape: `{kind:"onchain_view", chain:"$chain", contract:"$to", function:"sig(types)", decoder_id:"..."}` + `ttl_s`) | `registryV2/manifests/<p>/...` |

> A·B 의 match 들은 exhaustive(컴파일러가 누락 강제)지만, **D 카탈로그·C cedarschema field·E manifest source 는 silent** — 빠뜨리면 컴파일은 통과하고 decode-error 나 conformance 패닉으로 나타난다. ACTIONBODY_EXTENSION_GUIDE.md §2.5 가 touchpoint 별 코드 스니펫.

#### live_field 타입별 매핑 (★ U256 은 한 사례 — 환산값이 늘 수량은 아니다)
B/C/D 의 구체 형태는 `LiveField<T>` 의 `T` 에 따라 다르다. **`lending/supply.rs` 의 `SupplyLiveInputs` 가 전 변종(U256/Decimal/bool/struct)을 다 보여주는 정본** — 새 타입은 거기서 미러:

| `T` | Cedar type | D skeleton (`live_input_default`) | B apply coercion | C lowering (`.value`) |
|---|---|---|---|---|
| `U256` | `String` (hex) | `JsonValue::String("0".into())` | `value_to_u256(&v)` | `u256_hex(...)` |
| `Decimal` / `Price` | `String` | `JsonValue::String("0".into())` | `value_to_decimal(&v)` | `....to_string()` |
| `bool` | `Bool` | `JsonValue::Bool(false)` | `Value::Bool(b)` 매칭 | `Value::Bool(...)` |
| `Address` | `String` | zero-addr skeleton | `encode_address`/직접 | `addr(&...)` |
| struct (`ReserveState`·`UserLendingState` 등) | named Cedar type | `<struct>_skeleton()` (domain helper) | `serde_json::from_value(v)` | `lower_<struct>(&...)` |
| tuple/array (`(U256,U256)`·`Vec<_>`) | tuple/`Set` | `json!(["0","0"])` / `[]` | `serde_json::from_value` | per-element |

> `value_to_u256`/`value_to_decimal` = `action_walk/mod.rs` 공유 헬퍼. struct skeleton(`lending_reserve_state_skeleton()` 류)·`lower_<struct>` 는 해당 domain 의 기존 helper 미러. **golden 의 `source.function` pin(아래)은 `T` 무관**(source 메타는 값 타입과 독립) — 어느 타입이든 같은 방식으로 wiring 검증.

#### golden — 값이 아니라 source 를 pin
live_field 의 **값**은 host 가 채우므로(테스트는 skeleton `0`) corpus/golden 으로 값 정확성을 검증할 수 없다(정직). 대신 **manifest source 가 decode 까지 wired 됐는지**를 deterministic 하게 pin:
```rust
// expected_wsteth 객체로 scope → source.function 이 manifest 와 일치하나
let live = find_object_by_key(&env, "expected_wsteth").expect("...");
assert_eq!(find_string_field(live, "function").as_deref(), Some("getWstETHByStETH(uint256)"));
```
이게 깨지면 = manifest 가 live_inputs 를 drop 했거나 view 를 오타냈거나 LiveInputs 필드가 round-trip 안 됨. 값 정확성(환산이 맞나)은 §8.4 의 semantic 한계 영역 — host RPC 책임.

#### 빈 `live_inputs: {}` 가 정당한 경우 (defer 사유)
- 모든 디코드 필드가 이미 user-legible (erc20 amount, recipient address, bool flag).
- 환산 소스가 깔끔한 on-chain view 부재(예: staking APR 은 oracle report 기반) → 사유 명시 defer.
- 배열/tuple 인자 모델링이 과중(예: `getWithdrawalStatus(uint256[])→tuple[]`) → 다음 라운드로 defer + corpus `_note`.

비울 거면 **왜 비웠는지 한 줄**을 남긴다("decoded fields already in token units" / "APR source ambiguous, deferred"). 무근거 빈 `{}` = enrichment 미수행으로 간주.

---

## 5. P2 — TEST

### 5a. 하니스 구동 (V3 경로 단독)

하니스는 production export `declarative_route_request_v3_json` 을 plain Rust 로 직접 호출한다(브라우저/RPC/GCS 없음). `registryV2/index/` 전체를 설치 후 라우트.

```bash
cd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2

# 게이트 (CI 결정적, 4 test):
cargo test -p policy-engine-integration-tests --test v3_decode_harness -- --nocapture

# CLI 빌드(병렬 build-lock 회피 시 prebuilt 직접 호출):
cargo build -p policy-engine-integration-tests --bin v3-harness
#   → target/debug/v3-harness <sub>
cargo build -p policy-engine-integration-tests --bin check-onboarding-evidence
#   → target/debug/check-onboarding-evidence <protocol> --phase all
```

**CLI 서브커맨드** (`src/bin/v3_harness.rs`):
```bash
v3-harness fuzz [--iterations N] [--seed S] [--json PATH]   # 합성 전략별 sweep + 리포트
v3-harness coverage                                          # strategy별 callkey 수 + deferred
v3-harness replay --callkey <chain>__<addr>__<selector> [--seed S]   # 단건 재현
v3-harness corpus [--root DIR] [--filter <protocol>] [--require-expect-body]
                                                            # 실거래 corpus replay + expect/semantic pin 체크
v3-harness import-etherscan|import-dune|import <export.json> [--chain N] [--out PATH]  # parse-only
```
seed 기본 `0x5C09EBA1`. corpus root 기본 `data/golden/v3-decode/`. `--filter` 는 source path 에 대한 case-insensitive substring 이다(`curve` → `curve-router-ng/corpus.json` 등). 새 프로토콜 landing 은 `--filter <protocol> --require-expect-body` 를 반드시 추가해, 전체 corpus green 이 해당 프로토콜의 semantic field pin 부족을 가리지 못하게 한다.

**Evidence gate CLI** (`src/bin/check_onboarding_evidence.rs`):
```bash
check-onboarding-evidence <protocol> [--phase all|p0|p1|p2|p3|p4]
check-onboarding-evidence --path <evidence.md> [--phase all|p0|p1|p2|p3|p4]
```
`done`/`blocked` 만 phase row status 로 허용한다. 둘 다 artifact/summary cell 이 필요하고, `blocked` row 가 있으면 Blockers table 의 concrete row 도 필요하다.

**입력 DTO** (`dto.rs:198-227`, `DeclarativeRouteRequestV3InputDto`):
```jsonc
{ "chain_id": 1, "to": "0x..", "selector": "0x..", "calldata": "0x..",
  "value": "0", "gas_limit": "0", "gas_price": "0",
  "submitter": "0x..", "submitted_at": 1700000000, "nonce": 0, "block_timestamp": null }
```
value/gas_* 는 **10진 문자열**. 하니스 조립 = `harness/route.rs:15` `route_calldata`.

**게이트** (`tests/v3_decode_harness.rs`) = **4 structural + protocol 별 field-level golden 다수** (총수는 늘어난다 — 측정: `cargo test --test v3_decode_harness 2>&1 | grep "test result"`). 4 structural: `surface_installs_clean`(≥300 callkey · ≥50 bundle · 0 install-fail) / `synthetic_fuzz_single_emit`(0 hard) / `corpus_replay` / `synthetic_fuzz_all_strategies`(0 hard). 나머지 = hash/derived/live 필드를 pin 하는 golden(§9·§9.9·§9.10 류, corpus 가 못 보는 값).

**R1 (필수):** install state 는 thread-local → install 과 route 는 **같은 thread**. 새 헬퍼는 install→route 를 한 함수에서.

#### P2 synthetic — random fuzz + edge synthesis

목표 = **내가 고른 real tx 가 아니라, authored adapter surface 전체가 임의 입력과 boundary 입력에서 production decoder 를 깨지 않는지** 확인한다. Synthetic 은 입력값을 우리가 만들기 때문에 `amount`, `recipient`, `spender`, array length, opcode 같은 plumbing 을 inverse-check 하기 좋다.

**기본 random fuzz loop:**
```bash
cd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2
cargo build -p policy-engine-integration-tests --bin v3-harness

target/debug/v3-harness coverage
target/debug/v3-harness fuzz --iterations 5000 --seed 0x5C09EBA1 \
  --json crates/integration-tests/logs/<protocol>/$(date +%F)-synthetic.json
```

해석:
- hard failure 0 이 기본 gate. soft error 는 `oracle.rs` 의 `SOFT_ERROR_KINDS` 와 shape-artifact 규칙에 들어갈 때만 허용.
- 실패가 난 callkey 는 `target/debug/v3-harness replay --callkey <chain>__<addr>__<selector> --seed <seed>` 로 raw envelope 를 재현한다.
- protocol filter 가 없더라도 전체 surface 를 돌린다. protocol scoped run 이 필요하면 로그에서 `<protocol>` callkey 만 분리하거나 `validate --filter <protocol>` 를 author-time shape gate 로 병행한다.

**필수 edge synthesis:** random 은 boundary 를 우연히 충분히 맞히지 못한다. 모든 COVER selector 중 permission/value-bearing/nested/array/opcode/typed-data 는 hand edge 를 최소 1개 이상 만든다. 현재 자동 edge 주입이 부족하면 `data/golden/v3-decode/_edge-cases/corpus.json` 또는 `<protocol>/corpus.json` 에 대표만 추가한다.

Protocol-agnostic edge menu:
- **amount/value**: zero, one, max uint256, exact native `msg.value`, value=0 with payable calldata, value>0 with nonpayable-like path.
- **permission**: allowance zero revoke, max allowance grant, finite allowance, operator true/false, nonce invalidation min/max, expiry 0/max.
- **address role**: recipient == sender, recipient third-party, owner/on_behalf_of third-party, spender/operator nonzero, zero address only if protocol explicitly allows or expected error 로 pin.
- **array/tuple**: empty array expected error, singleton, two items, max practical small N, mismatched parallel array lengths expected error.
- **router/nested**: one child, multiple children, unsupported helper child, mixed supported/unsupported child, truncated inner calldata expected error.
- **opcode/discriminant**: first/last known opcode, unknown opcode expected error/excluded, enum discriminant out of case expected soft artifact only if would-revert.
- **path bytes**: shortest valid path, multi-hop path, malformed length expected error.
- **deadline/time**: expired-looking timestamp and far-future timestamp only if decoder should preserve not evaluate.

Edge corpus rule:
- committed corpus 는 대표 case 만 보관한다. raw random dump 금지.
- every permission/value-bearing selector must have at least one edge entry or field-level golden.
- edge `expect:"pass"` 는 `expect_body` 로 semantic-critical field 를 pin 한다. edge `expect:"error"` 는 `expect_error` 를 pin 한다.

### 5b. 데이터 fetch (real-tx, adapter-BLIND)

목표 = **어댑터를 무시하고** 프로토콜의 실거래를 무작위로 받아 디코드가 정확한지 본다(어댑터 보고 tx 고르지 않음).

#### Etherscan txlist (bulk 주력)
**한 API call 이 현재 최대 10,000 tx 를 반환하고 각 tx 의 `input` 필드 = calldata** 다. 즉 목표는 **프로토콜당 최소 10,000 tx**, 10,000 API call 이 아니다. 단, Etherscan 공지상 Free tier 의 일부 endpoint 최대 record 수는 **2026-07-01 부터 1,000/request 로 축소 예정**이므로 새 온보딩 세션은 항상 현재 docs 를 재확인한다. `.env` 의 Etherscan API 기준 일 100,000 call 여유가 있어도 무작정 call 을 태우지 않는다. call budget 은 address × block-window 수로 기록하고, 10k tx 와 selector/shape 포화가 먼저다.
```bash
set -a; source crates/integration-tests/.env; set +a   # ETHERSCAN_API_KEY (commit 금지)
curl -s "https://api.etherscan.io/v2/api?chainid=1&module=account&action=txlist&address=<ADDR>&page=1&offset=10000&sort=desc&apikey=$ETHERSCAN_API_KEY" -o /tmp/es.json
target/debug/v3-harness import-etherscan /tmp/es.json --chain 1 --out /tmp/v3probe/<protocol>/corpus.json
target/debug/v3-harness corpus --root /tmp/v3probe        # 격리 probe
```
- import = **parse-only offline**. column pick: `to`←[to,to_address,contract_address], `data`←[data,**input**,calldata], value default "0". 모든 엔트리 기본 `expect:"pass"` 로 찍히니, probe 후 got 분포를 보고 expect 를 재배치.
- **status 무필터**(calldata 구조만 검증, revert 무관).
- **제약:** ① 주소당 한 쿼리 window 는 현재 최대 10,000 record(2026-07-01 이후 Free tier 는 1,000 예정) → 더 깊이는 `startblock/endblock` block-range 페이지네이션. ② free tier rate 는 현재 3 req/s, 100,000 calls/day(과잉). ③ **free tier txlist 가 Base(8453)/Optimism(10) 등 거부**("Free API access is not supported for this chain") → 그 체인의 real-tx 는 Dune/유료. ABI/source endpoint 는 Etherscan docs 상 모든 supported chain 에서 Free 가능. ④ recency bias(sort=desc).
- proxy `eth_getTransactionByHash` 는 value 가 16진 → corpus 는 10진 필요. **txlist 권장.**

**Etherscan run policy (per protocol):**
1. P0 의 `cover` contract address 별로 txlist 를 adapter-blind 하게 가져온다. manifest selector 를 보고 고르지 않는다.
2. 최소 floor = **10,000 tx/protocol** 또는 selector×shape 포화. 단 모든 COVER selector 는 real tx sample ≥1 을 우선한다.
3. recency-only bias 를 줄이려면 `sort=desc` 한 번으로 끝내지 말고, 필요한 경우 `startblock/endblock` 을 여러 window 로 나눈다.
4. raw export 는 `/tmp` 또는 `logs/<protocol>/raw-*` scratch 로만 둔다. committed corpus 는 dedup representative + failure/excluded/high-value pass 만.
5. tx pull target address count 를 기록하고, 0이면 성공으로 치지 않는다. `_deployments.json`/surface schema mismatch, wrong status field, chain filter, or empty universe bug 로 보고 수정한다.
6. evidence ledger 에 `api_calls_used`, `raw_txs_seen`, `unique_selectors_seen`, `covered_selectors_with_real_tx`, `low_traffic_or_absent_selectors`, representative tx hashes/corpus path 를 기록한다. 이 행이 비어 있으면 P2 real-tx 는 미완이다.

#### Dune (핀포인트 보완 — 조건부)
selector 필터(`WHERE ...`)·decoded 테이블·cross-chain·빈도 통계용. credit 기반(community plan **2,500 credit/월**; `getUsage`(또는 MCP `mcp__dune__getUsage`)로 확인 — billing period 월간, **일일 캡 아님**).
- **실측 비용 (2026-05-31, `community_fluid_engine_v2`)**: pruned pinpoint 쿼리(`block_time ≥ now()-interval '1' day` + `to=` 필터, LIMIT 50, **`performance:"free"`**) = **0.007 credit/execution** (응답 `result_preview.resultMetadata.executionCostCredits` + usage delta 2중 확인). → 2,500 / 0.007 ≈ **~35만 쿼리/월** = **credit 은 pinpoint 의 binding constraint 아님**.
- **★ 비용 = datapoints *scanned***: 0.007 은 (a) free 엔진 + (b) **파티션 컬럼(`block_time`/`block_date`/`block_number`)에 WHERE → partition pruning** 덕. **partition 필터를 빼면 풀스캔 → 한 쿼리가 수백~수천 credit** 태워 월 예산 즉사(`LIMIT` 있어도 스캔 후 자름). **항상 partition WHERE + 좁은 window + free 엔진.**
- **프로토콜당 할당**: ration 불필요 — 수십 pruned 쿼리 = ~0.1~2 credit/protocol. **tripwire ~25 credit/protocol**(초과 = partition 필터 누락 풀스캔 의심 → 즉시 점검). 2,500 의 대부분은 *실수 방어 buffer*.
- **lane**: bulk 볼륨은 Etherscan(credit 무관). Dune 은 **free-Etherscan 미지원 체인(Base 8453 / OP 10)** · cross-chain join · decoded 테이블 · selector 빈도 통계 — 여기서만 Dune 이 유일(§5d C).

**Dune MCP calibration (first run per environment):**
1. `getUsage` 로 credit baseline 을 기록한다.
2. free engine + partition WHERE + narrow window 로 `LIMIT 100`, `LIMIT 1000`, `LIMIT 5000` probe 를 실행해 rows/sec, result cap, credit delta 를 기록한다.
3. 같은 쿼리에서 partition WHERE 를 넓히기 전 `EXPLAIN`/metadata 또는 usage delta 를 확인한다. credit jump 가 보이면 즉시 중단.
4. evidence ledger 에 `dune_rows_returned`, `executionCostCredits`, `usage_delta`, `window`, `chain`, `selector_filter`, query id/SQL summary, selected tx hashes 를 남긴다.
5. Dune target 은 "Etherscan 이 못 하는 chain/selector 에 real tx ≥1" 이 1차다. MCP/free tier 로 10k tx 를 안정적으로 가져올 수 있음이 probe 로 확인된 경우에만 Dune 도 bulk 로 사용한다.
6. Dune 을 전혀 호출하지 않았으면 "Dune skipped" 라고 쓰지 말고 `blocked_external_data:dune` 또는 `not_applicable:<reason>` 을 ledger 에 남긴다. cross-chain/decoded-stats/Base/OP 중 하나라도 관련되면 Dune 은 not-applicable 이 아니다.

#### Stratify (무작정 N 늘리지 말 것)
버그는 **(selector × arg-shape) 커버**에서 나온다. 10,000 random 이면 selector 20종 프로토콜의 shape 를 거의 포화시킨다. 같은 N 이라도 **selector 전수 + block-range 분산**이 recency-only 보다 낫다. **포화 지표**: "1,000건당 새 distinct shape 수" → 0 에 수렴하면 stop(더 돌리면 compute 낭비). raw 영속화 금지 — **실패 + dedup 대표만** corpus 로.

### 5c. Hybrid Oracle (정확성 판정)

**현재 하니스가 하는 것 (oracle.rs `judge`):** shape + domain 까지만.
- L1 Envelope(ok 필드) / L2 TypedRoundTrip(`Vec<simulation_reducer::action::Action>` 역직렬화) / L3 Domain(`VALID_DOMAINS`) / L4 ErrorClass.
- `VALID_DOMAINS` 는 `crates/integration-tests/src/harness/oracle.rs` 를 직접 재확인. 새 domain 추가 시 §4a 의 downstream contract 와 함께 동기화한다.
- `SOFT_ERROR_KINDS`(tolerate) = no_declarative_v3_mapper, unsupported_strategy_for_typed_data, no_typed_data_mapper.
- corpus `expect` 는 `expect_domain` 을 비교하고, 선택적 `expect_body` 가 있으면 필드값도 비교한다. `expect_action` 은 아직 reserved.

→ **즉 현 oracle 은 "안 죽고 올바른 모양" 까지(coverage). 필드값(token/amount/spender)이 맞는지는 안 본다.** "정확하게 파싱" 을 달성하려면 아래 hybrid 를 **이 방법론이 추가로 요구**한다(일부는 하니스 확장 필요 — 구현 시 명시).

#### 3계층 (정답 작성량 = selector 수 비례, tx 수 무관)

**① 자동 바닥 (정답 0, 전 tx 자동)**
- `coverage`: 라우팅됐나? (uncovered 버킷)
- `decode-no-error`: alloy 디코드 성공? (decode-error 버킷)
- **provenance**: ActionBody 의 모든 스칼라(주소/uint)가 calldata 디코드 값 집합에 존재하나? → 값 **날조/손상** 적발. (현 하니스 미구현 → 추가 시 production `DecodedCall` 값 ⊆ ActionBody 값 체크. alloy 독립 디코드 불필요 — production decode 결과 재사용.)

**② B — per-selector projection (random emit-accuracy 의 핵심, future harness work)**
selector 당 **정답 매핑 공식 1개**를 manifest 와 **독립**으로 작성 → 그 selector 의 모든 random tx 자동 대조.
```jsonc
// data/golden/v3-decode/<protocol>/projections/<selector>.json  (방법론 prescribed 포맷)
{ "selector": "0x095ea7b3", "abi": "approve(address spender,uint256 amount)",
  "expect": { "domain": "token", "action": "erc20_approve",
    "token":   "$tx.to",       // approve 대상 = 토큰 컨트랙트 자신
    "spender": "$rawarg.spender",   // alloy 독립 디코드 결과(manifest emit 재사용 금지)
    "amount":  "$rawarg.amount" } }
```
- **비순환성 규칙(생명):** projection 의 기대값은 **raw ABI 디코드 + 독립 작성**이어야 한다. manifest 의 emit 규칙을 복사하면 "어댑터가 어댑터를 검증" → 아무것도 못 잡음. projection 은 2nd-opinion(divergence 적발)이지 증명이 아님.
- 비용 = selector 당 1회. covered 고트래픽 selector 부터.
- 현 하니스에는 projection executor 가 아직 없다. 현재 landing 은 `expect_body` corpus / field-level Rust golden 으로 semantic-critical field 를 pin 하고, projection 은 구현 후 strict gate 로 승격한다.

**③ A — hand-authored golden (정밀 spot-check, 수십 건)**
까다로운 대표 tx(nested/edge)만 **사람이 기대 ActionBody 를 직접** 적어 corpus 로. corpus 포맷(아래 5e)의 `expect_body` 로 필드 레벨 기대를 추가한다. ABI+emit 전부 잡지만 tx당 1정답이라 **curated 수십 건**(blind random 엔 부적합).

#### 1,000 vs 10,000 tx — 정답 작성량 동일
```
N tx (selector 20종):
  자동 바닥(coverage/decode/provenance) → N건 자동, 정답 0
  B projection 20개                     → N건 emit 자동 대조
  A golden ~30개                         → 30 정답 손작성
정답 = 0 + 20 + 30 = 50  (N=1k 이든 10k 이든 동일)
```

### 5d. Scale 정책 + 소스별 하한 (coverage floor)

**★ 하한은 "tx 수" 가 아니라 "selector × shape 커버리지" 다** (oracle ∝ selector, §2). 아래 floor 를 채우되 **shape 포화 지표(1,000건당 새 distinct shape → 0)** 면 tx 수 못 채워도 stop. 영속화 = 실패 + dedup 대표만(raw 덤프 금지).

| 소스 | 하한 (lower bound) | stop 조건 | 디테일 |
|---|---|---|---|
| **A 합성** | 모든 COVER callkey 가 machine fuzz(4 전략) + protocol-agnostic edge menu 적용; **permission·value-bearing selector 마다 hand-edge ≥1** (infinite-approval / zero-amount / empty / truncated calldata) | callkey별 shape 포화 | seeded fuzz(`--seed`, 재현) + A-2 `_edge-cases/corpus.json` 손수(§5a P2 synthetic) |
| **B Etherscan** (bulk 주력) | free-지원 체인(mainnet 등): **모든 COVER selector 가 real-tx decoded sample ≥1** (저트래픽/부재면 corpus `_note` 로 "low-traffic/absent" 명기) · 기본 target **10,000 tx/protocol** stratified | shape 포화 **또는** 10k tx 도달 | txlist 현재 최대 10k tx/API call(Free tier 2026-07-01 이후 1k 예정 — docs 재확인) · adapter-blind · **selector 전수 + block-range 분산**(recency-only 지양) · api_calls_used 로그 |
| **C Dune** (조건부) | **필수 조건**: 프로토콜이 free-Etherscan 미지원 체인(Base/OP)에 배포 **또는** cross-chain/decoded-stats 필요 → 그 chain/selector 의 real-tx ≥1. **그 외엔 optional**(skip 가능, Etherscan 으로 충분) | 해당 chain/selector real-tx 확보, 또는 calibration 이 bulk 가능 입증 | MCP calibration 먼저: usage baseline/delta + LIMIT 100/1000/5000 probe + partition WHERE. bulk 는 안전 확인 후 |

- **B vs C 분담**: 볼륨·기본 10k 는 **B**(무료, credit 무관). **C** 는 B 가 *못 하는 갭*(Base/OP·cross-chain·decoded)만 — credit 은 충분하나 partition 규율 필수(§5b).
- compute/fetch/authoring 다 N 에 거의 무관 → 위 floor 넘겨 올려도 되나, **shape 포화면 marginal 가치 0**(더 돌리면 compute/credit 낭비).
- **정직한 한계**: 위 floor 는 *covered* selector 기준. research(P0)가 못 찾은 selector/컨트랙트는 floor 대상에도 안 들어옴 → 그건 §3 I0/I1 gate 의 몫(테스트 floor 로는 못 잡음).

### 5e. Verdict 버킷 + 로그

각 tx 결과를 자동 분류:
| 버킷 | 의미 | 처치(P3) |
|---|---|---|
| **correct** | 디코드 + 두 계층 통과 | — |
| **MIS-DECODED** | 디코드됐는데 필드 불일치 | emit 수정(manifest) 또는 엔진(4c) |
| **uncovered** | 어댑터 없음 (soft no_declarative_v3_mapper) | manifest 추가(4b) |
| **decode-error** | hard 실패 (build_*_failed 등) | abi_fragment 또는 엔진(4c) |

**corpus 엔트리 포맷** (`corpus.rs CorpusTx`, `data/golden/v3-decode/<protocol>/corpus.json`):
```jsonc
{ "transactions": [
  { "intent": "swap", "expect": "pass", "expect_domain": "amm",
    "expect_body": [
      { "path": "$.data.actions[0].body.domain", "op": "equals", "value": "amm" }
    ],
    "tx_hash": "0x..", "chain_id": 1,
    "rpc": { "params": [ { "to": "0x..", "value": "0", "data": "0x.." } ] } },
  // 또는 EIP-712:
  { "expect": "excluded", "chain_id": 1,
    "typed_data": { "verifying_contract": "0x..", "primary_type": "PermitSingle", "domain_name": "Permit2", "message": { } } }
] }
```
- `expect`: `"pass"`(ok + expect_domain 일치 + expect_body 일치) / `"excluded"`(ok + domain=unknown, off-chain 정상) / `"error"`(Fail|Soft + expect_error 일치).
- `expect_body`: JSON Pointer(`/...`), `$` dotted/index path(`$.data.actions[0]`), recursive field path(`$..address`) 지원. op = `exists`, `absent`, `equals`, `not_equals`, `one_of`, `contains`, `len`, `nonzero_address`, `hex_eq`, `u256_hex_eq`.
- 로그: `crates/integration-tests/logs/<protocol>/YYYY-MM-DD-<source>.json` (포맷·인덱스 = `crates/integration-tests/README.md` §6 "Log→Gap→Develop 루프" 따름). 매 실행이 직전과 diff 되도록 커버리지 매트릭스 + gaps + sample_failing_txs + summary.

---

## 6. P3 — DEVELOP 루프

로그의 각 gap 을 분류 → 처치 위치로 보냄 → 재테스트. (위 5e 표가 분류→처치 매핑.)

```
for each gap in logs/<protocol>/:
  uncovered     → 4b manifest 추가
  MIS-DECODED   → 4b emit 필드 수정  또는  4c 엔진 확장
  decode-error  → 4b abi_fragment 수정  또는  4c 엔진
  domain 모름   → 4a schema 추가  (또는 Unknown 허용)
  → logs 갱신 → P2 재실행 (corpus expect flip)
포화(새 gap 0)까지 반복.
```
큰 Tier 2/Tier 3 건은 상세 fix-plan 후 안전 착지 범위까지, 안 되면 `expect:error` baseline + `_note` defer + 문서화(정직).

---

## 7. P4 — LAND

```bash
cd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2

# 1) index 재빌드 (manifest 변경 반영). deterministic — manifest 무변이면 0 churn.
cd registryV2
npm run build
npm run check:manifest                 # CI-safe: representative sourced index + source-ref representative validate
npm run check:surface
# pool/factory/vault-heavy protocol only:
npm run check:universe -- --protocol <protocol> --require-cover-linkage

# exhaustive manifest validation is intentionally separate from CI-safe check:manifest.
# Run when feasible (local/nightly/protocol-scoped), otherwise record resource blocker in evidence.md.
npm run check:manifest:full
cd ..

# 2) corpus expect flip (해결된 gap: error→pass) 후 회귀
target/debug/v3-harness corpus
target/debug/v3-harness corpus --filter <protocol> --require-expect-body
cargo test -p policy-engine-integration-tests --test v3_decode_harness
cargo test --workspace                 # 0 fail

# 3) Tier 2 변경 시 WASM
./scripts/wasm-build.sh                 # 사이즈 ≤ 6 MiB 점검

# 4) lint
cargo clippy -p policy-engine-integration-tests --all-targets && cargo fmt --all
```

**커밋 규율:**
- **explicit-stage only** (`git add -A` 금지). 대상 = `registryV2/manifests/<p>/**` · (재빌드 후) 해당 `registryV2/index/` 추가분 · Tier 2 Rust · Tier 3 schema · `crates/integration-tests/{logs,data/golden}/**`.
- **절대 제외**: 무관 churn(browser-extension/index curation 등), `.env`(ETHERSCAN_API_KEY 로컬만).
- ⚠️ **`cargo fmt --all` 함정**: base 에 unformatted-committed 파일이 있으면(타 세션/머지 잔재) `fmt --all` 이 **내 파일이 아닌 것도 재포맷** → 무관 churn. fmt 후 `git status` 로 내가 안 건드린 파일이 보이면 stage 하지 않는다. 실제 revert 는 명확히 내가 만든 변경이거나 사용자 승인을 받은 경우에만 한다.
- 메시지 말미: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

## 8. 부록

### 8.1 커맨드 치트시트
```bash
# 게이트
cargo test -p policy-engine-integration-tests --test v3_decode_harness -- --nocapture
# 합성 soak + 리포트
target/debug/v3-harness fuzz --iterations 5000 --json /tmp/v3-report.json
# 커버리지(strategy 분포)
target/debug/v3-harness coverage
# 실거래 corpus
target/debug/v3-harness corpus [--root DIR] [--filter <protocol>] [--require-expect-body]
# 단건 재현
target/debug/v3-harness replay --callkey 1__0x<addr>__0x<sel> --seed 0x5C09EBA1
# import (parse-only)
target/debug/v3-harness import-etherscan /tmp/es.json --chain 1 --out data/golden/v3-decode/<p>/corpus.json
# index 재빌드
cd registryV2 && npx tsx scripts/build-index.ts && cd ..
```

### 8.2 ActionBody domain 카탈로그 (요약)
token · amm · lending · airdrop · launchpad · perp · liquid_staking · permission · staking · multicall · unknown (11). (각 domain action 목록 = §4a 표. **작성 전 `<domain>/mod.rs` 직접 확인** — 도메인·스키마 둘 다 확장됨.)

### 8.3 알려진 함정 (DEFECT_CATALOG.md, V3 관점)
- **nested tuple per-component 타입 유실** (D010 류): `[i][j]` 접근 시 uint width 정보 유실 → string 화 → u64 coercion 실패. **Permit2 류는 commit `3f93f5c` 에서 해결**(chained-numeric + coercion). 새 프로토콜 nested-tuple 에서 재발 가능 → 같은 패턴 점검.
- **schema enum drift** (D007): manifest 가 쓰는 enum 값(예 approvalKind `"erc721"`)이 Rust enum 에 없으면 deserialize fail → 양쪽 동기화.
- **fork 프로토콜 struct/opcode layout** (D006/D010): Pancake PoolKey(6 field) vs Uniswap V4(5 field) 처럼 fork 가 layout 다름 → **1차 출처 직접 fetch** 로 field index 검증. mirror 가정 금지.
- **dotted path**: `$args.x.y` 미지원 → chained-numeric `$args.x[i][j]`.
- **존재 안 하는 domain target**: schema 없는 domain/action 을 manifest 가 가리키면 hard-fail (Unknown 으로 안 떨어짐). schema 먼저.
- **빈 `live_inputs: {}` ≠ 온보딩 완료** (§4d/§8.6-5): 디코드는 성공해도 추상 단위(shares/index/wrapped)는 사용자에게 안 읽힌다. 게이트가 전부 green 이어도 enrichment 누락은 자동으로 안 잡힌다 — author 가 §8.6-5 로 의식 점검.
- **U256 직렬화 = lower-hex** (golden 작성 함정): `U256` 는 JSON 에 **lower-hex** 로 나간다(`9_800_000_000_000_000_000` → `"0x88009813ced40000"`). golden 에서 decimal 문자열로 assert 하면 실패 — 디코드 결과를 한 번 출력해 실제 직렬화 형태를 확인 후 pin.

### 8.4 정직한 한계
- **semantic 층은 객관 오라클이 없다.** "path[0]이 진짜 token-at-risk 인가", "domain=amm 인가" 는 ScopeBall 의미모델이 정의 — 체인이 안 알려줌. provenance(값 출처)·B projection(2nd-opinion)까지가 자동 검증의 한계. 필드 **역할** 정합성은 사람(또는 미래의 시뮬레이션 trace)이 보증.
- **synthetic vs real-tx 오라클 분담:** synthetic 은 입력값을 알아 plumbing 을 inverse-check(객관), 단 내가 만든 shape 만. real-tx 는 shape 다양성(객관 = ABI/provenance), 단 emit semantic 은 projection(2nd-opinion)까지.
- **데이터 한계:** recency bias, free Etherscan 의 Base/OP 미지원, HyperEVM(chain 999) 등 Etherscan 미지원 체인.
- **file:line 은 작성 시점 스냅샷.** 코드는 움직인다(이 매뉴얼 작성 중에도 V1 retire/Permit2 fix 가 일어났다) — **작업 전 grep 재확인**.

### 8.5 이 매뉴얼이 가정하는 코드 상태 (재검 기준점)
- declarative 레이어 = `action_builder.rs` + `args_json.rs` + `types.rs` (V1 interpreter 제거됨, commit `4e60392`).
- V3 dispatch = `declarative_exports.rs:530` `match strategy` (single_emit / opcode_stream_dispatch / array_emit / tagged_dispatch / multicall_recurse / 그 외 unsupported).
- 빌더 = `action_builder.rs` build_action_body(514) / build_multicall_from_opcode_stream(966) / build_array_emit(1048) / substitute_placeholders(238).
- 위가 안 맞으면 코드가 또 움직인 것 — §1 표를 `grep -n` 으로 갱신 후 진행.

### 8.6 completeness self-check (P0/P1 마무리 게이트)
온보딩 "완료" 선언 전 8문 — 하나라도 No 면 미완. 0·1·5 가 build 강제(컨트랙트·주소·함수 전수성), 2~4·6~7 은 산문 자가점검(enrichment/token-surface 는 build-gate 가 부분적):

**컨트랙트 인벤토리 (가로의 전제 — P0, §3 규약 8):**
0. 프로토콜의 **deployed 컨트랙트를 전수**했나? (공식 deploy 목록 1차 + DefiLlama/Dune/Etherscan-labels sweep) — `surface/<protocol>/_deployments.json` 에 모든 컨트랙트를 cover/exclude:reason 으로 적고 **`check:surface` 의 I0 가 PASS** 인가? ⚠️ 이게 빠지면 I1(함수)·실거래 pull(주소 기반) **둘 다 못 잡는** 컨트랙트-레벨 누락이 남는다. floor: 공식 목록만큼만 완벽(§3 규약 8).

**주소 유니버스 (pool/factory/vault-heavy P0):**
1. 유저가 직접 호출하는 factory/pool/vault child 주소 universe 를 닫았나? `surface/<protocol>/_address_universe.json` 또는 `_pool_universe.json` 에 nonzero source count 와 모든 candidate 의 `cover|exclude|defer:reason` 이 있고, **`npm run check:universe -- --protocol <protocol>` 가 PASS** 인가? P4 에서는 `--require-cover-linkage` 로 모든 `cover` 주소가 generated callkey 를 갖는지도 확인했나?

**가로 (surface — P0):**
2. hub 의 **external state-changing 함수를 전수**했나? (block explorer Write 탭 / interface 전체 — "눈에 띄는 것만" 아님)
3. 각 함수를 **COVER 또는 EXCLUDE:reason** 로 분류했나? (분류 안 된 함수 = 0)
4. 모든 **approval·permit·delegation·authorization grant**(on-chain + off-chain EIP-712)를 잡았나? Unknown/skip 0?
5. **`npm run check:surface` 가 PASS 인가?** `surface/<protocol>/<contract>.{abi,coverage}.json`(verified 전체 ABI snapshot + per-selector triage)을 작성하면 gate 가 I1~I3·S1/S2 를 기계 검증 — 위반 시 exit 1 = P0 미완. 2~4 를 사람이 빠뜨려도 독립 snapshot 이 잡는다. (§3 규약 7 / `registryV2/surface/README.md`)

**토큰 인벤토리 (P0/P1 — §3.2):**
6. 프로토콜이 만들거나 직접 다루는 **LP/share/receipt/debt/governance/base token** 을 `crates/integration-tests/TOKEN_INVENTORY_GUIDE.md` 기준으로 조사했나? `registryV2/tokens/<chain>/<addr>.json` 에 필요한 토큰과 underlying ref 를 등록했고, large pool protocol 의 제외분을 P0 로그에 명시했나? 이게 빠지면 ERC 표준 callkey auto-enumerate 가 token address 를 못 만들어 표준 `approve/transfer/transferFrom` 을 놓친다.

**세로 (enrichment — P1, §4d):**
7. **각 COVER action 의 디코드 필드가 user-legible 한가?** 추상·불투명·간접 단위(shares / 내부 index / wrapped·rebasing 수량 / rate-dependent)를 가진 필드마다 (a) 사용자 단위로 바꾸는 live_field 를 달았거나, (b) **manifest 에 한 줄 사유로 defer 를 명시**했나? 무근거 빈 `live_inputs: {}` = No. ⚠️ 이 질문은 객관 오라클이 없어 **build 로 강제 못 한다**(§8.4) — author 가 의식적으로 통과시켜야 하는 prose gate. "게이트 다 green = 완료"의 함정이 여기 산다.

> **반례 세 개.** 가로: §9 1차에 supply/withdraw/borrow/repay 4 만 보고 `setAuthorization`(권한 위임)을 놓침 → 이제 4(`check:surface`)가 `I1 un-triaged selector 0xeecea000` 으로 build 실패(실측). 토큰: ZRO ERC20 이 `tokens/` 에 없어 표준 ERC20 transfer/approve 가 `no_declarative_v3_mapper` 로 빠짐 → token JSON 추가 후 auto-enumerate 로 해결. 세로: §9.9 Lido 1차에 6 action 전부 `live_inputs: {}` 로 착지 — 게이트는 다 green 인데 `transferShares` 의 `shares`(추상 단위)가 사용자에게 안 읽혀 **피드백으로만** 잡힘. 6 이 있었으면 author 가 작성 시점에 잡았다.

---

## 9. Worked Example: Morpho Blue P0→P4 (실측 transcript)

> 이 매뉴얼을 **dogfood** 으로 검증하며 만든 실제 온보딩 기록. 모든 커맨드·출력은 실측(2026-05-30, branch `feat/registry-v2`). 결과물은 commit `760af8c` 로 영구 온보딩됨(manifest 4 + Tier B Rust + golden corpus + golden test).
> Morpho Blue 를 고른 이유: lending 이고 `LendingVenue::MorphoBlue` 가 schema 에 이미 존재(=Tier 3 불필요) — "manifest+test 90% 케이스"로 보였다. **그런데 `market_id` 가 keccak 해시라 manifest-only 로 안 됐다.** 이 갭이 이 예시의 핵심 — manifest→Tier B 에스컬레이션과, 자동 오라클이 못 잡는 **silent MIS-DECODED** 를 코드로 실증한다.

### 9.1 P0 — Research (1차 출처)
주소·ABI 는 `docs.morpho.org` + `github.com/morpho-org/morpho-blue@v1.0.0` 1차. selector 는 **local keccak**(self-test 로 도구 검증).
```text
singleton (mainnet) = 0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb
struct MarketParams { address loanToken; address collateralToken; address oracle; address irm; uint256 lltv; }
MarketParamsLib.id = keccak256(marketParams, 5*32)   // = keccak256(abi.encode(..)) (static struct)
```
```bash
$ cast sig 'transfer(address,uint256)'                 # 도구 self-test
0xa9059cbb                                              # ✓ (= 알려진 값)
$ cast sig 'supply((address,address,address,address,uint256),uint256,uint256,address,bytes)'
0xa99aad89
# withdraw 0x5c2bea49 · borrow 0x50d8cd4b · repay 0x20b76e81
# (미커버) supplyCollateral 0x238d6579 · withdrawCollateral 0x8720316d · setAuthorization 0xeecea000 · createMarket 0x8c1358a2 · liquidate 0xd8eabcb8
```

### 9.2 P1 — Author (schema → manifest)
**9.2a schema 확인** — `reducer/src/action/lending/mod.rs`: `LendingVenue::MorphoBlue { chain, market_id: String }` + supply/withdraw/borrow/repay action 전부 존재 → **Tier 3 변경 0**. 단 `market_id` 가 **keccak 해시 String** 임을 발견 — placeholder 문법(index/slice 만, hash 불가)으로 못 만든다.

**9.2b manifest** — `registryV2/manifests/morpho/morpho-blue/{supply,withdraw,borrow,repay}@1.0.0.json` (single_emit, aave/v3 선례 mirror). 매핑: `asset=$args.marketParams[0]`(loanToken, tuple→positional array), `amount=$args.assets`, `on_behalf_of=$args.onBehalf`, `market_id="$derived.morpho_market_id"`(= 설계 의도; deriver 는 P3 에서). live_inputs 는 각 LiveInputs struct 필드명과 일치(skeleton, prod-fill).

### 9.3 P2 — Test (데이터 fetch + 3단계 corpus 진행)
**adapter-blind pull** — singleton 의 최근 400 직거래를 무작위로(어댑터 고려 없이) 가져온다.
```bash
$ curl -s "https://api.etherscan.io/v2/api?chainid=1&module=account&action=txlist&address=0xbbbb…effcb&offset=400&sort=desc&apikey=$KEY" -o raw.json
# selector 분포: withdraw 101 · repay 60 · borrow 47 · supply 40  (= covered 248)
#               withdrawCollateral 60 · supplyCollateral 44 · setAuthorization 35 · createMarket 7 · liquidate 3 · native 3  (= uncovered 152)
$ target/debug/v3-harness import-etherscan raw.json --chain 1 --out /tmp/morpho/corpus.json
wrote 400 transactions to /tmp/morpho/corpus.json     # import 는 전부 expect:"pass" 로 찍음
```
corpus 를 **세 번** 돌린다 — 각 단계가 gap 버킷 하나씩 보여준다(테스트=루프 엔진, §2):
```bash
# RUN #1 — build-index 전 (Morpho 가 index 에 아직 없음) = DISCOVERY iter-0
$ target/debug/v3-harness corpus --root /tmp/morpho
corpus: 0/400 matched
# 400 전부 MISS, got=soft(no_declarative_v3_mapper)            ← 전부 UNCOVERED

# build-index → Morpho callkey 4개 생성
$ (cd registryV2 && npm run build)
… done — 468 callkey(s) … across 94 manifest(s)

# RUN #2 — build-index 후, Tier B 전
$ target/debug/v3-harness corpus --root /tmp/morpho
corpus: 0/400 matched
#  152 MISS  got=soft(no_declarative_v3_mapper)                ← 여전히 UNCOVERED (미작성 selector)
#  248 MISS  got=FAIL[ErrorClass] build_action_body_failed:
#            unknown placeholder (no fallback): derived.morpho_market_id   ← DECODE-ERROR (loud)
```
RUN #2 의 248 = supply/withdraw/borrow/repay. manifest 가 라우팅되기 시작했으나 `$derived.morpho_market_id` 가 미해소 → **hard error**(silent 아님 — 시스템이 거부). 이게 Tier B 가 필요한 신호.

### 9.4 P3 — Develop (Tier B + golden + silent MIS-DECODED 실증)
**Tier B** — `declarative_exports.rs` 에 `maybe_inject_morpho_market_id` + `compute_morpho_market_id`(= `maybe_inject_v4_pool_id` 미러; address×4 left-pad + uint256 lltv = 0xa0 버퍼 → `keccak256`). single_emit 의 `ctx.derived` 에 주입(decode 직후, `match strategy` 전).
```bash
$ cargo build --bin v3-harness                          # policy-engine-wasm 재컴파일
# RUN #3 — Tier B 후 (corpus 는 아직 전부 expect:pass)
$ target/debug/v3-harness corpus --root /tmp/morpho
corpus: 248/400 matched                                 # supply/withdraw/borrow/repay → PASS/lending ✓
#  152 MISS = no_declarative_v3_mapper (미커버, 정상)    ← corpus 주석으로 expect:"error" 처리
```
**market_id 정확성 cross-check** — 실제 supply tx 1건의 marketParams 를 **독립 도구**로 검산(내 hand-rolled 버퍼와 별개 구현):
```bash
$ cast abi-encode "f((address,address,address,address,uint256))" "(0xC02a…,0xe1B4…,0xcb6a…,0x870a…,915000000000000000)" | cast keccak
0xb7ad412532006bf876534ccae59900ddd9d1d1e394959065cb39b12b22f94ff5   # = MarketParamsLib.id (= 기대값)
```
**field-level golden `#[test]`** (`morpho_supply_market_id_is_keccak_marketparams`) — real supply tx 를 `route::route_calldata` 로 라우팅, ActionBody 의 `market_id == 위 keccak` assert. **corpus 가 절대 못 잡는 값**을 잡는 유일 수단.

**silent MIS-DECODED 실증** — manifest 의 `market_id` 를 일부러 `$args.marketParams[0]`(loanToken, **틀렸지만 valid String**)로 바꿔 보면:
```text
$ target/debug/v3-harness corpus … | grep <supply-tx>
  ok   … expect=pass got=pass                            ← corpus 는 통과!! (verdict+domain 만 봄, corpus.rs:204)
$ cargo test … morpho_supply_market_id_is_keccak_marketparams
  left:  "0xc02aaa39…c756cc2"   (틀린 loanToken)
  right: "0xb7ad4125…94ff5"     (정확한 keccak)
  test … FAILED                                          ← golden 만 잡아냄
```
> **이게 매뉴얼 §8.4 의 핵심 한계를 코드로 보여준 것.** 자동 오라클(coverage+decode+domain)은 semantic 으로 틀린 디코드를 `ok` 로 통과시킨다. field-level golden(=hybrid oracle 의 "A" 층)만이 잡는다. (실증 후 manifest 원복 → 전부 green.)

### 9.5 P4 — Land
```bash
$ cargo test --workspace                                # 전 크레이트 0 fail (v3_decode_harness 5 passed)
$ git add  registryV2/manifests/morpho/**  registryV2/index/by-callkey/1__0xbbbb…__0x{a99aad89,5c2bea49,50d8cd4b,20b76e81}.json \
           crates/policy-engine-wasm/src/declarative_exports.rs  crates/integration-tests/tests/v3_decode_harness.rs \
           crates/integration-tests/data/golden/v3-decode/morpho/corpus.json
$ git diff --cached --name-only | wc -l                 # 정확히 11 (전부 Morpho) — build-index 의 무관 index churn 미포함
$ git commit …                                           # 760af8c (Co-Authored-By 푸터)
```
**explicit-stage 주의(§7):** `npm run build` 는 `index/by-callkey/` 를 광범위 재생성 → working tree 의 무관 churn 과 섞인다. Morpho **신규** callkey 4개는 untracked(`??`)라 구분 가능 → 그것 + 내 4 manifest + 2 Rust + corpus 만 stage. `git diff --cached` 로 무관 churn 0 확인 후 commit.

### 9.6 이 dogfood 이 입증한 것
- **방법론이 실제로 돈다**: P0(1차출처)→P1(schema 확인+manifest)→P2(blind pull+3단계 corpus)→P3(Tier B+golden)→P4(workspace 0 fail+surgical commit) 가 멈춤 없이 관통.
- **manifest-only 90% 가정은 깨질 수 있다**: keccak 파생 필드(market_id, pool_id 류)는 Tier B Rust 필수. 선례(`maybe_inject_v4_pool_id`)를 먼저 찾아 미러하면 bounded.
- **자동 오라클의 semantic 갭은 실재한다**(§8.4): corpus 는 verdict+domain 까지. 필드값 정확성은 **반드시 field-level golden** 으로 못박아야 — 안 그러면 틀린 market_id 가 `ok` 로 영원히 통과. 새 프로토콜의 hash/derived 필드마다 golden 1건 권장.
- **수치**: manifest 4 + Tier B helper 2 fn(~50 LOC) + golden 1 + corpus 13건. authoring 비용 ∝ selector 수(여기 6, 작성 4) — tx 수(400)와 무관(§2).

### 9.7 Surface-completeness 보강 — Full 8 (사용자 지적 → 매뉴얼 dogfood)

§9 1차는 supply/withdraw/borrow/repay **4만** 보고 끝냈는데, 사용자가 "Morpho 함수가 더 많을 텐데?" 지적. 재triage(§3 보강된 P0)로 **user surface = 8** 확정 — 그중 `setAuthorization`(권한 위임)이 ScopeBall 핵심인데 1차에서 uncovered 로 버려졌었다.

**전수 triage (IMorpho.sol v1.0.0, external 17):**

| class | 함수 | triage |
|---|---|---|
| user-fund-move | supply/withdraw/borrow/repay | COVER (§9.1~6) |
| user-fund-move | supplyCollateral/withdrawCollateral | COVER (B1) |
| **permission-grant** ⚠️ | setAuthorization(on-chain) / Authorization(off-chain EIP-712) | **COVER (B2, Tier 3)** |
| keeper·infra | liquidate/flashLoan/accrueInterest/createMarket | EXCLUDE |
| relayer-submit | **setAuthorizationWithSig** (서명자=user 의 off-chain 서명을 제3자 relayer 가 제출; 서명자 risk 는 off-chain Authorization sign 시점에 포착) | EXCLUDE |
| governance(onlyOwner) | setOwner/enableIrm/enableLltv/setFee/setFeeRecipient | EXCLUDE |

**B1 collateral (쉬움, schema 변경 0)**: supplyCollateral→`Supply{asset=$args.marketParams[1](collateralToken)}`, withdrawCollateral→`Withdraw{asset=marketParams[1], recipient=receiver}`. 기존 action 재사용 + market_id Tier B 그대로. corpus 248→**352**.

**B2 setAuthorization (Tier 3 — 핵심 escalation)**: 기존 LendingAction 에 "operator 전권 위임" action 부재 → Unknown 으로 떨구면 과소경고(§8.4 핵심 실패) → **신규 `LendingAction::SetAuthorization`** (bespoke `{chain,protocol,authorized,is_authorized}` locator; market 무관이라 venue 없음). 확장가이드 ①~⑦ + **등록 3 site**: `SHIPPED_SCHEMA_FILES` + `REGISTERED_ACTIONS` + **`per_policy.rs` ActionEntry 테이블**.
> ⚠️ **확장가이드 갭 발견**: `ACTIONBODY_EXTENSION_GUIDE.md` 가 manual ⑤를 `SHIPPED_SCHEMA_FILES` 만 명시했으나, 실제론 `REGISTERED_ACTIONS` + `per_policy.rs` ActionEntry 테이블(+그 `len()` assertion)도 등록해야 lowering schema 가 합성된다. conformance test 가 `MissingAction` 으로 잡아줘서 발견(안전망 작동). → 확장가이드 보강 필요(별건).

on-chain manifest(selector `0xeecea000`) → corpus 352→**387**. 나머지 13 = createMarket/liquidate/native(excluded).

**off-chain Authorization (8번째, pre-sign 핵심)**: 유저가 서명하는 EIP-712 `Authorization{authorizer,authorized,isAuthorized,nonce,deadline}` (relayer 가 `setAuthorizationWithSig` 로 제출). typed-data manifest — flat scalar 5개 → `build_typed_data_args_json` 의 flat 경로 → `$args.authorized`/`$args.isAuthorized` 직접. **단 Morpho EIP-712 domain 은 minimal(chainId+verifyingContract, name/version 없음)** → build-index 가 `domain_name 필수` 로 reject → **`build-index.ts` 검증을 optional 로 완화**(witness_type "when present" 패턴 미러; nameless EIP-712 허용, backward-compatible, 기존 manifest 영향 0). by-typed-data 라우팅(verifying_contract+primary_type) → probe **1/1**.

**검증**: `cargo test -p policy-engine`(252) / `-p simulation-reducer`(403) / `v3_decode_harness` **6 passed** (+`morpho_set_authorization_decodes_operator_and_flag` golden: operator `0x4A6c312e…` + grant flag pin — corpus 가 "누가 권한받나"를 미검증하는 층). 커밋 corpus 17건(15 pass/lending + 2 error) green.

**교훈**: 보강된 §3 surface-completeness gate + permission red-flag(Part A)가 이 누락을 1차에 잡았을 것. 권한 grant 는 Unknown 이 아니라 Tier 3(§4a override). ScopeBall 가장 핵심 함수가 manifest-only 가 아니라 schema 확장을 요구했다 — "쉬운 4개"만 집으면 정확히 그걸 놓친다.

**후속(executable gate, 2026-05-31)**: "잡았을 것"은 산문 self-check 의존이라 약하다 → §3 규약 7 의 `npm run check:surface` 로 **기계 강제**. 사용자 진단("함수별 어댑터를 못 작성했으면 research logic 결함")이 정확 → research-completeness 를 trust 에서 build-enforced invariant 로 승격. **gate 가 만들면서 진짜 잔여 gap 도 드러냈다**: 이 §9.7 triage 표가 실제로 **setAuthorizationWithSig 를 누락**(16 나열 / Etherscan verified 17) — on-chain `setAuthorization` + off-chain `Authorization` 만 잡고 17번째 on-chain `setAuthorizationWithSig`(0x8069218f)는 명시 triage 안 됨. 독립 ABI snapshot 의 I1 이 강제 → 명시 결정(EXCLUDE: relayer-submit). 산문 triage 는 사람이 16 을 17 로 착각해도 통과하지만, snapshot diff 는 불가능. 상세 = `registryV2/surface/README.md` + `surface/morpho/morpho-blue.{abi,coverage}.json`.

### 9.8 Compound V3 Comet P0→P4 kickoff (2026-05-31)

Compound V3 Comet(mainnet cUSDCv3, `0xc3d688b66703497daa19211eedff47f25384cdc3`)도 같은 절차로 dogfood 했다. P0는 `registryV2/surface/compound-v3/comet-usdc-mainnet.{abi,coverage}.json`으로 고정했고, gate 기준 external-mutating 20개를 전수 triage했다: `supply*`/`withdraw*`/`transfer*`/`buyCollateral`/`allow`/`allowBySig`/`approve`/`approveThis` = COVER, governance/infra/keeper path = EXCLUDE. 특히 Comet `approve(spender,amount)`는 ERC20 approve처럼 보이지만 Comet manager authorization으로 해석해야 하므로 permission red-flag COVER로 분류했다.

P1에서 기존 lending action으로 표현되지 않는 `buyCollateral`은 `LendingAction::BuyCollateral` Tier 3로 추가했다. `allowBySig`와 off-chain `Authorization`은 signer 보존이 필요해서 `SetAuthorization.authorizer?: Address`를 추가했다. 또 Comet cUSDCv3가 token auto-enumeration과 protocol-specific manifest를 동시에 갖는 충돌을 드러냈다: standard ERC20 sourced manifest가 Comet `approve/transfer/transferFrom`을 덮으면 permission semantics가 silent downgrade된다. 그래서 `build-index.ts`와 WASM install bridge를 "concrete protocol manifest wins"로 보강했다.

P2/P3 검증은 field-level golden 3개(`allow`, `allowBySig`, `Authorization`)와 hand-encoded corpus 16건으로 pin했다. `npm run check:surface`는 Comet `20 surface · 15 cover · 5 exclude · 15 on-chain manifests · 1 signed-struct`로 PASS, `v3_decode_harness`는 10 tests green, `v3-harness corpus`는 `132/132 matched`. 한계: 이 checkout에는 `ETHERSCAN_API_KEY`가 없어 Compound corpus는 Etherscan txlist import가 아니라 verified ABI 기반 hand fixture다. 따라서 P0/P1/Tier3/gate/field-golden은 완료됐지만, 실거래 corpus augment는 후속 작업으로 남긴다.

### 9.9 Lido enrichment — "게이트 green 인데 user-illegible" (§4d/§8.6-5 의 반례)

Lido(Liquid Staking)는 §4d ENRICHMENT 단계가 **왜 필요한지**를 실증한 사례다. 이 절은 그 dogfood 의 transcript이자, enrichment 가 없을 때 무슨 일이 나는지의 증거다.

**1차 온보딩 (commit `ad15b48d`)** — 축1 새 `liquid_staking` domain(stake/wrap/unwrap/request_withdrawal/claim_withdrawal/transfer_shares) + manifest 12. **모든 게이트 green**: check:surface PASS(stETH/wstETH/WQ 전수 triage), check:manifest 12 OK, corpus 9 pass, golden 3, workspace 0 fail. set_authorization 선례("decode-faithful, live_inputs 불필요")를 따라 **6 action 전부 `live_inputs: {}`** 로 착지.

**그런데 사용자 피드백이 필요했다** (= 방법론 결함의 신호):
1. "어댑터 live_field 전부 비어있는데 의도된거야?"
2. (SR02 `exactOutputSingle` 의 채워진 live_inputs 를 가리키며) "이렇게 작성해야 하는 거 아니냐"
3. **결정적**: "SR02 와 *동일하게* 쓰라는 게 아니라, 이 구조(`transferShares`)에 **필요한** live_field 가 필요하다" — `transferShares(recipient, sharesAmount)` 의 `shares` 는 **프로토콜 내부 share 단위**라, 디코드는 정확해도 사용자는 "내가 얼마(stETH)를 보내는지" 를 못 본다.

**진단 = §4d 가 빠져 있었다.** 디코드는 faithful 했으나(게이트 통과), §8.6-5(enrichment-completeness)에 해당하는 질문 — "이 필드가 user-legible 한가" — 을 방법론이 묻지 않았다. §3 Morpho `setAuthorization` 누락(가로)과 **같은 구조의 결함의 세로 버전**.

**처치 (commit `934d775c`)** — 환산 3종(wrap `expected_wsteth`=`getWstETHByStETH` / unwrap `expected_steth`=`getStETHByWstETH` / transfer_shares `pooled_eth`=`getPooledEthByShares`). **★ `getPooledEthByShares(shares)` 는 calldata 인자(shares)를 view 로 넘겨야 하므로 manifest-only 불가** → §4d 의 5-touchpoint 전부:
- A `WrapLiveInputs{expected_wsteth: LiveField<U256>}` 등 (reducer)
- B `ActionSlot::LiquidStakingTransferSharesPooledEth` + `action_walk/liquid_staking.rs`(walk/apply) + `args_resolver` arm(`encode_u256(t.shares)` — `encode_u256` 는 `fetchers/decoder.rs` 에 이미 있었음)
- C cedarschema `pooledEth: String` + lowering `.value` + test_support `live_u256()` skeleton
- D `live_input_default` 3 entry
- E manifest `live_inputs.pooled_eth.source = {onchain_view, getPooledEthByShares(uint256), $to}`

**golden = source.function pin**(값은 host): `lido_wrap_expected_wsteth_live_input_is_wired` 가 `find_object_by_key(env,"expected_wsteth")` → `function == "getWstETHByStETH(uint256)"` assert. 검증: workspace 0 fail, v3_decode_harness 35 pass, check:manifest 1022 OK, check:surface PASS. claim_withdrawal(`getWithdrawalStatus` tuple[]) / stake(APR 소스 모호) / request_withdrawal(amounts 이미 token 단위)는 §4d 규칙대로 **사유 명시 defer**.

**교훈**: enrichment 는 게이트로 안 잡힌다(객관 오라클 부재) — author 가 §4d decision-tree + §8.6-5 self-check 로 **작성 시점에** 잡아야 한다. "디코드 성공 + 게이트 green" 은 "사용자가 intent 를 읽는다"를 보장하지 않는다. 추상 단위(shares/index/wrapped/rate)를 가진 모든 새 action 이 이 함정의 후보다.

### 9.10 contract-inventory gate (I0) — "못 찾은 컨트랙트는 테스트도 장님" (§3 규약 8 dogfood)

§3 의 surface gate(I1)와 P2 의 실거래 pull 은 둘 다 **리서치가 찾은 컨트랙트 집합에 갇힌다**: I1 은 snapshot 안 뜬 컨트랙트엔 돌 대상이 없고, 실거래 pull 은 `txlist&address=<리서치가 준 주소>` 라 못 찾은 주소를 query 조차 안 한다. 즉 **컨트랙트-레벨 누락은 두 안전망 다 못 잡는다** — 이건 사용자 지적("dune/etherscan 이 주소 기준이면 리서치가 못 찾은 주소는 테스트로도 못 잡는 거 아니냐")이 정확했던 실제 blind spot.

**처치 = I1 패턴을 한 층 위로 (commit 은 이 절과 함께).** 함수의 ground-truth 가 verified ABI 이듯, **컨트랙트의 ground-truth 는 공식 deployment 목록**. `surface/lido/_deployments.json` 에 `docs.lido.fi/deployed-contracts`(1차, WebFetch)의 **전체 15 컨트랙트**를 전수 triage: stETH/wstETH/WithdrawalQueue = `cover`(snapshot 보유), 나머지 12(Lido impl / Staking Router / Locator / Accounting / Withdrawal Vault / Oracle 3 / DAO Kernel / LDO / Dual Governance / Timelock) = `exclude:reason`(impl-behind-proxy / infra / oracle / governance / standard-ERC20). `check:surface` 의 **I0** 가 모든 cover 의 snapshot 존재를 강제:
```
✓ [I0] lido: 15 deployed · 3 cover · 12 exclude (contract-inventory enforced vs docs.lido.fi/deployed-contracts)
⚠ aave/compound-v3/curve/morpho/uniswap: contract-inventory NOT enforced (no _deployments.json)   ← opt-in WARN, 비파괴
```

**vacuous 아님을 negative test 로 증명**: `Staking Router`(snapshot 없음)를 `exclude→cover` 로 뒤집자 →
```
✗ I0 surface/lido/_deployments.json: deployment "Staking Router (proxy)" (1/0xfddf…2999) is COVER but has NO surface snapshot/coverage — research missed a user-facing contract …   (exit 1)
```
즉 리서치가 user-facing 컨트랙트를 놓치고 그걸 cover 로 적으면 build 가 막는다 (되돌려 green).

**정직한 floor**(§3 규약 8): verified ABI 는 함수를 못 빠뜨리지만 deployment 페이지는 컨트랙트를 누락할 수 있다 → I0 는 "공식 목록만큼" 완벽(I1 보다 약함). SPOF 를 "agent 기억"→"공식 목록 + DefiLlama/Dune/Etherscan-labels cross-check"로 옮길 뿐. 그래서 I0 는 opt-in WARN(강제 아님) — 목록을 작성해야 닫힌다.

<!-- 출처: 사용자 설계 세션(V3-only/4-phase/3-tier/hybrid oracle/10k scale) + 코드 grounding(action_builder.rs·declarative_exports.rs·args_json.rs·dto.rs·oracle.rs·corpus.rs·v3_harness.rs·실제 manifest 5종·action/**·DEFECT_CATALOG.md) + §9 dogfood 실측(Morpho Blue 4함수 commit 760af8c + Full-8 보강: collateral 2 + SetAuthorization Tier 3 + off-chain Authorization). 2026-05-31. -->
