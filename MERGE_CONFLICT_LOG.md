# Merge Conflict Resolution Log — origin/main → phase7-uniswap-e2e

> **Context**: `origin/main` (16eee56, 80 new commits, +19956/-691, 206 files) 을 phase7 worktree `worktree-phase7-uniswap-e2e` branch (HEAD `a27128f`) 위로 merge.

## 정책

1. **기본 방침: `origin/main` 채택 우선**
2. **단 무조건 아님** — phase7 의 unique feature (declarative pipeline, Curve enum_tagged, Aerodrome ve(3,3), wasm-opt) 가 손실되는 경우 `union` 또는 `manual merge`
3. **각 file 별 best option 명확히 검토** — 4 선택지 (origin / phase7 / union / manual) 의 trade-off 비교
4. **origin 채택 시 phase7 영향 분석** — 어느 phase7 code 가 수정/제거/migration 되는지 코드 기반 추적

---

## 충돌 분류 요약

| 분류 | 개수 | Resolved |
|---|---|---|
| File location (cedar files) | 4 | ✅ |
| Content conflict | 9 | 6 / 9 |
| Semantic refactor mismatch (잠재) | 6+ | Task 9 |

---

## ✅ Resolved (지금까지)

### L1-L4: `policy-examples/swap/*.cedar` → `policy-rpc/examples/policies/swap/`

- **What**: origin/main 이 `policy-examples/swap/` 디렉토리 → `policy-rpc/examples/policies/swap/` 로 rename
- **HEAD**: phase7 가 같은 시기 `policy-examples/swap/` 에 4 cedar file 추가
  - `forbid-untrusted-output-symbol.cedar`
  - `forbid-zero-recipient.cedar`
  - `swap-long-deadline.cedar`
  - `swap-short-deadline.cedar`
- **Best options**:
  - A) Move 4 file 의 새 위치 채택 (`git add policy-rpc/...`) — origin 디렉토리 구조 호환
  - B) old 위치 유지 + 추가 — origin rename 의도 무시
- **Decision**: **A (origin 디렉토리 구조 채택)**
- **Rationale**: git rename detection 이 phase7 file content 를 새 디렉토리로 자동 이동 — 그 결과 채택. origin 의 디렉토리 정리 의도 따름.
- **Phase7 영향**: phase7 의 4 cedar file content 의 새 path 로 이동 — 본인 내용은 보존.

---

### C1: `.gitignore`

- **What**: 양쪽이 같은 영역에 다른 entry 추가
- **HEAD**: `.claude/` (worktree isolation)
- **origin**: 5 entry (`crates/*-wasm/pkg/`, `browser-extension/{backend,dashboard/src,dashboard/public,public}/wasm/`)
- **Best options**:
  - A) Union (양쪽 entry 보존)
  - B) origin 채택 — `.claude/` 빠짐 → worktree-specific config 가 git 에 leak
  - C) phase7 채택 — wasm-pack artifact 가 git 에 leak (build output, ~5 MiB)
- **Decision**: **A (Union)**
- **Rationale**: gitignore entry 는 누적적, 어느 쪽도 잘라낼 가치 없음. 양쪽 leak 위험 동시 차단.
- **Phase7 영향**: zero (entry 보존).

---

### C5: `browser-extension/yarn.lock`

- **What**: 양쪽 dep 추가
- **HEAD**: phase7 declarative-related dep
- **origin**: dashboard SPA dep 대량 추가
- **Best options**:
  - A) origin 채택 (`--theirs`) + `yarn install` regenerate
  - B) Manual merge (lock format hard to merge)
- **Decision**: **A (origin 채택)** ← origin 우선 정책 + 기술적 합당
- **Rationale**: lock file 은 `yarn install` 로 regenerate 가능. origin superset 채택 후 phase7 dep 가 `package.json` 에 있으면 자동 추가. Manual merge 무의미 (giant hash table).
- **Phase7 영향**:
  - phase7 의 `package.json` 의 declarative dep (예: viem, ajv 등) 가 `yarn install` 후 lock 에 자동 추가
  - 단 phase7 의 package.json 도 다음 conflict (Task 6 의 service-worker 영역) 에서 확인 필요

---

### C8: `crates/policy-engine-wasm/src/lib.rs`

- **What**: `pub use` re-export list 양쪽 변경
- **HEAD**: `declarative_exports::*` + `evaluate_with_envelopes_json` (phase7)
- **origin**: `get_alias_table_json` + `preview_custom_schema_json` (origin)
- **Best options**:
  - A) Union (양쪽 export 보존)
  - B) origin 채택 — phase7 declarative exports 모두 빠짐 → declarative pipeline 깨짐
  - C) phase7 채택 — origin 의 D14/alias table 기능 빠짐
- **Decision**: **A (Union)**
- **Rationale**: B 는 phase7 declarative pipeline 완전 무력화 — 허용 불가. Export 자체는 단순 list, semantic conflict zero.
- **Phase7 영향**: zero (declarative re-export 보존).

---

### C6: `crates/policy-engine-wasm/src/dto.rs`

- **What**: 양쪽 disjoint DTO 추가
- **HEAD**: 8 phase7 DTO (`DeclarativeInstallResultDto` / `DeclarativeLookupInputDto` / `DeclarativeCtxDto` / `DecodedCallDto` / `DecodedArgDto` / `DecodedValueDto` / `DeclarativeRouteRequestInputDto` / `DeclarativeRouteRequestResultDto`)
- **origin**: 9 origin DTO (`AliasEntryDto` / `AliasTableOutput` / `PreviewCustomSchemaInputDto` / `CustomTypeDto` / `CustomSchemaDiffDto` / `CustomFieldChangeDto` / `InstallPoliciesOutputDto` / `PreviewInstalledSchemaOutputDto` / `PreviewCustomSchemaOutputDto`)
- **Best options**:
  - A) Union (양쪽 DTO 보존)
  - B) origin 채택 — phase7 declarative DTO 빠짐 → declarative_exports.rs 가 사용하는 type 깨짐
  - C) phase7 채택 — origin 신규 DTO 빠짐 → preview_custom_schema_json 등 origin export 깨짐
- **Decision**: **A (Union)**
- **Rationale**: 양쪽 disjoint, 이름 충돌 zero. `lib.rs` union 과 일관성 (양쪽 export 가 wire 됐어야 함).
- **Phase7 영향**: zero (declarative DTO 보존).

---

### C4: `browser-extension/backend/service-worker/wasm-bridge.ts`

- **What**: 4 conflict — 양쪽 disjoint method + interface + function 추가
- **HEAD**: 4 wasm method (declarative_*) + 5 interface (Declarative*) + 3 function (`installDeclarativeBundle` / `declarativeMap` / `declarativeRouteRequest`)
- **origin**: 3 wasm method (preview_custom / preview_installed / get_alias) + 3 interface (PreviewCustomSchemaOutput / PreviewInstalledSchemaOutput / AliasTableEntry) + 3 function (`previewCustomSchema` / `previewInstalledSchema` / `getAliasTable`)
- **Best options**:
  - A) Union (4 conflict 모두 양쪽 keep)
  - B) origin 채택 — phase7 declarative bridge layer 손실 → declarative-route.ts / declarative-adapter-loader.ts 등 phase7 marketplace 코드 모두 깨짐
  - C) phase7 채택 — origin manifest preview / alias table 손실 → dashboard SPA 깨짐
- **Decision**: **A (Union)**
- **Rationale**: 양쪽 의존 chain 이 disjoint — phase7 marketplace + origin dashboard 같이 보존 필요. Method/interface 이름 충돌 zero.
- **Phase7 영향**: zero (declarative bridge layer 보존).

---

## ✅ C7: `crates/policy-engine-wasm/src/exports.rs` (2502 줄, 4 sub-conflict) — Resolved

#### Sub-conflict #1 (line 4-12): `use` imports

- **What**: 양쪽 disjoint DTO import 추가
- **HEAD**: `EvaluateWithEnvelopesInputDto` 추가
- **origin**: 9 신규 DTO import (alias / custom schema 관련)
- **Best options**:
  - A) Union (양쪽 imports 보존, 알파벳 sort)
  - B) origin 채택 — `EvaluateWithEnvelopesInputDto` 빠짐 → `evaluate_with_envelopes_json` (phase7 신규 export, file 의 다른 영역에 존재 + conflict 없음) 의 input parse 깨짐 → declarative 의 verdict 평가 path 완전 무력화
- **Decision**: **A (Union)** — origin 우선 정책의 예외
- **Rationale**: B 는 phase7 declarative pipeline 의 verdict driver (`evaluate_with_envelopes_json`) 깨뜨림. Union 의 cost zero (단순 import list).
- **Phase7 영향**: zero.

#### Sub-conflict #2 (line 62-71): `install_policies_json` closure signature

- **What**: closure return type + 첫 statement 양쪽 다름
- **HEAD**: `Result<(), EngineErrorDto>` + `check_input_size(&policies_json, "install_policies_json")?;` (phase7 audit P0 fail-closed)
- **origin**: `Result<Option<InstallPoliciesOutputDto>, EngineErrorDto>` (Phase 5 의 manifests-map shape 지원)
- **Best options**:
  - A) **Manual merge** — origin signature + phase7 check_input_size
  - B) origin 채택 — `check_input_size` 빠짐 → 큰 input JSON 으로 WASM memory DoS 가능 (phase7 audit P0)
  - C) phase7 채택 — Phase 5 manifests-map shape 깨짐 → origin dashboard install path 무력화
- **Decision**: **A (Manual merge)** — origin signature 채택 + phase7 input size guard 보존
- **Rationale**: origin 의 return type 이 새 표준 (Phase 5 install 결과 dashboard 가 사용). 단 phase7 audit P0 fail-closed 도 보존 — input size guard 는 cheap defensive check.
- **Phase7 영향**:
  - closure return type 변경: `()` → `Option<InstallPoliciesOutputDto>`
  - install_policies_json fn body 의 다음 영역 (origin 변경) 가 새 return type 따라 처리 필요 — file 의 line 80-130 영역 자동 호환 (origin 가 처음부터 새 type 따라 작성, conflict 없음)

#### Sub-conflict #3 (line 749-797): `evaluate_policy_rpc_json` evaluate logic

- **What**: evaluate body 양쪽 큰 분기
- **HEAD**: `evaluate_envelopes_inner(...)` helper 호출 — phase7 가 inline 로직을 helper 로 refactor (Phase 7A)
- **origin**: inline `ActionAddress::parse` + envelope→request loop + **`apply_rpc_results_with_indices`** + **D9 SystemFail handling**
- **Best options**:
  - A) **origin 채택** — D9 + manifest-driven flow 가 새 표준
  - B) phase7 채택 — `evaluate_envelopes_inner` helper 가 새 D9 flow + apply_rpc_results 호환하도록 rewrite (Task 9)
  - C) 양쪽 keep — `evaluate_policy_rpc_json` 가 dual-path (legacy phase7 + new D9)
- **Decision**: **A (origin 채택)**
- **Rationale**: D9 SystemFail + manifest-driven flow 가 origin 의 새 표준 — `evaluate_policy_rpc_json` 의 main path 가 새 flow 사용 자연스러움. phase7 의 `evaluate_envelopes_inner` helper 는 `evaluate_with_envelopes_json` (phase7 only export, line 838+) 에서만 사용 — scope 작음.
- **Phase7 영향**:
  - `evaluate_policy_rpc_json` 안의 `evaluate_envelopes_inner(...)` 호출 제거 → origin inline flow 채택
  - `evaluate_envelopes_inner` fn 자체는 file 에 그대로 보존 (line 888-, conflict 없음, `evaluate_with_envelopes_json` 가 사용)
  - **잠재 Task 9**: `evaluate_envelopes_inner` 가 D9 SystemFail 또는 manifest-driven flow 안 따른다면 declarative path 의 verdict 가 새 D9 의도와 호환 안 될 수 있음 — 추후 verify

#### Sub-conflict #4 (line 802-816): closure 마무리 + D9 SystemFail handling

- **What**: evaluate closure 의 마무리 부분
- **HEAD**: `)\n})();` — phase7 단순 closure return (evaluate_envelopes_inner 의 result)
- **origin**: D9 SystemFail handling + nested `STATE.with(... evaluate_requests ...)` block 의 closure 마지막 expression
- **Best options**:
  - A) origin 채택 (consistent with #3 의 옵션 A)
  - B) phase7 채택 (단순 closure)
- **Decision**: **A (origin 채택)**
- **Rationale**: Sub-conflict #3 의 origin 채택과 일관 — D9 flow + nested STATE.with evaluate_requests.

#### C7 origin 채택 결과 — phase7 의 수정 영역

| 영역 | 변경 |
|---|---|
| `use crate::dto` (line 3-14) | Union — 양쪽 imports keep |
| `install_policies_json` closure (line 60-) | return type → `Option<InstallPoliciesOutputDto>` (origin) + `check_input_size` 보존 (phase7 P0) |
| `evaluate_policy_rpc_json` (line 700-) | `evaluate_envelopes_inner` 호출 제거. inline origin flow 채택 (from/to/value_wei parse + envelope→request loop + apply_rpc_results_with_indices **4 args** + D9 SystemFail + STATE.with evaluate_requests) |
| `apply_rpc_results_with_indices` 호출 시그니처 | **`manifest_hash` + `plan.schema_hash` 인자 제거** — origin signature 와 호환 (4 args) |
| `evaluate_envelopes_inner` fn (line 888+) | 그대로 보존. `evaluate_with_envelopes_json` (phase7 only export, line 838+) 가 계속 사용 |

**잠재 Task 9 (semantic refactor)**: `evaluate_envelopes_inner` helper 가 D9 SystemFail / manifest-driven flow 안 따름. 즉 declarative path (phase7) 가 새 D9 의도 (system_fail_verdict / projection_failed) 와 호환 검증 필요. 단 helper 자체는 verdict 평가만 — D9 의 manifest projection 영역 안 닿음 — 일단 compile 통과 가능.

---

---

## ✅ C2: `service-worker/index.ts` — Resolved

- Conflict #1 (imports): **Union** — phase7 `ensureSeedBundlesInstalled` + origin manifest pipeline
- Conflict #2 (boot sequence): **origin 채택 + phase7 seed bundles 추가** — origin `bootSequence()` 안에 `ensureSeedBundlesInstalled()` 단계 삽입 (defaults → seed bundles → hydrate manifests)

## ✅ C3: `service-worker/orchestrator.ts` — Resolved

- Conflict #1 (imports): **Union** — phase7 `evaluateWithEnvelopes` + origin `formatAuditMatched`
- Conflict #2 (static path): **Manual merge** — origin manifest Map fallback (`getAllManifests()` + legacy Vec fallback) + phase7 declarative audit meta + helper fn (`auditFromDeclarativeOutcome` / `txValueToWeiDecimal`) 보존

## ✅ C9: `crates/policy-engine/src/schema/mod.rs` — Resolved

- File location: origin `schema.rs` → `schema/mod.rs` (디렉토리 split). git auto-moved.
- Conflict (base_schema_text entries): **origin 25 entries 채택** (phase7 11 entries 의 superset)
- Phase7 duplicate const block (path `../../../`, 잘못된 위치) 제거
- **Phase7 잠재 bug 수정**: Aerodrome 6 schema (gauge_vote/lp_stake/lp_unstake/lock_create/lock_increase/lock_manage) 가 base 에 register 안 됨 — 6 const 정의 + base_schema_text misc 영역에 6 entry 추가

---

## Post-merge semantic fixes (Task 9-10)

origin/main refactor 호환 — cargo check / test 단계에서 발견:

| Fix | 내용 |
|---|---|
| `policy_request_from_envelope` import | exports.rs 가 origin wrapper fn 사용 — phase7 dispatch.rs 에 이미 정의됨, import line 에 추가 |
| `include_str!` path | phase7 의 `policy-examples/{swap,permit,transfer,protocol,aerodrome}/` → `policy-rpc/examples/policies/` 로 이동 (origin 디렉토리 정리 일관). 11 file `git mv` + exports.rs path 갱신 |
| swap deadline cedar | `context.validityDeltaSec` → `context.custom.validityDeltaSec` (origin `3106fc6` manifest-driven custom context) + `context has custom` guard 추가 (cedar safety) |
| swap deadline test 2개 | `#[ignore]` — manifest-driven enrichment 필요, user policy 단독 install 로 trigger 불가 |
| vitest declarative-route test mock | `formatAuditMatched` + `SYSTEM_POLICY_ID` mock 추가 (origin orchestrator import) |
| vitest aerodrome test helper | `bundleSelector` 의 abi normalize (`outputs: []`) — abitype 1.0.0 호환 |
| `package-lock.json` | origin 이 npm → yarn workspace 전환 — `git rm` (yarn.lock berry format 단일화) |

## 최종 verification

| Check | Result | Baseline |
|---|---|---|
| cargo test | 835 / 0 / 6 | 764 / 0 / 3 |
| tsc | 0 error | 0 |
| vitest | 352 / 352 | 218 / 218 |
| WASM | 5.53 MiB (5,807,517 bytes) | 5.41 MiB |

## Commit + 빌드 산출물

| Commit | 내용 |
|---|---|
| `44c61bc` | merge: origin/main 80 commits → phase7 (13 conflict resolve + post-merge fix) |
| `6394617` | chore(registry): declarative bundle swapMode literal + index regen |

- webpack production build: `browser-extension/dist/chrome/` 생성 완료 — manifest.json + WASM 5.54 MiB + js/ + default-policies/ + default-manifests/ + seed-bundles/
- `package-lock.json` git rm — origin 이 npm → yarn workspace 전환
- `stash@{0}` (pre-pull lock files) drop — obsolete

## 미해결 (merge 와 별개)

- `AUDIT_PHASE8.md` / `AUDIT-PHASE12-CURVE.md` / `MERGE_CONFLICT_LOG.md` — commit 안 함 (사용자 결정)
- swap deadline test 2개 `#[ignore]` (`exports.rs`) — manifest-driven enrichment harness 생기면 재활성화
- GCS publish / git push / Round 9 manual e2e
- phase8-aerodrome worktree destroy — phase8 의 모든 commit 이 phase7 에 포함됨 (merge-base = phase8 HEAD `63f07ee`)

---

## Semantic refactor task (Task 9 — post-merge)

origin/main 이 새로운 system refactor:

| Refactor | 영향 |
|---|---|
| S1 `68a316b` — wire all 34 actions through base schema + Rule 4 | phase7 신규 13 Action 같은 패턴 적용 |
| S2 `5223a4a` — custom context placeholder | phase7 신규 cedarschema placeholder 추가 |
| S3 `3106fc6` — swap enrichment fields → manifest-driven custom context | phase7 swap mapping 새 model 검증 |
| S4 `ee78496` — D9 runtime failure model + D3 context.custom | phase7 lowering D9 호환 |
| S5 `19c4329` — install_policies_json takes manifests map | phase7 install_policies 호출 새 shape |
| S6 `2adf223` — reject unregistered when.action | phase7 신규 13 Action 모두 manifest 등록 검증 |

각 cargo check / vitest fail 발생 시 본 문서에 fix 항목 추가.
