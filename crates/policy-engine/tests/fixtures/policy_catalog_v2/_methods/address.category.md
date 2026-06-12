# method: address.category

status: aspirational (referenced by ~11 compliance/risk-category 정책; method-catalog.json 미등재 ·
POLICY_RPC_METHODS.md §3c 미열거 — 구현 시 둘 다 등록)

> 이 문서는 **interface 재진술이 아니라 구현 지침**이다. wire shape 와 `$.`-selector 의 SSOT 는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1 fold · §2 record→scalar
> projection · §3 method 표 · §6 selector 규약) 이고, 여기서는 *어떻게* 그 결과를 만들어내는지
> (데이터 소스 · 알고리즘 · 캐싱 · 실패 처리)를 정한다. **단, 작성 시점 SSOT(§3c)에는 `address.category`
> 항목이 아직 없다** — 이 method 는 reputation(이진 flag)과 구분되는 **typology 라벨** 메서드로 새로
> 추가돼야 하며, 본 spec 이 그 추가의 근거다. 모든 사실 진술은 1차 출처 기반이며, 미확인은
> "출처 미확인" 으로 명시한다.

> **`address.reputation` 와의 구분 (혼동 금지)**: `address.reputation` 은 "악성으로 *등재됐는가*" 를
> `flagged: Bool` 한 비트로 투영해 **deny** 를 트리거한다. `address.category` 는 상대방의 **risk
> typology 라벨**(mixer / sanctioned / ransomware …)을 `riskCategory: String` 한 개로 투영한다.
> 정책은 그 라벨을 분류 비교(`== "mixer"` 등)해 주로 **warn** 을 낸다. 두 method 는 데이터 소스가
> 겹칠 수 있으나(같은 분석 공급자), **반환 모양 · projection type · verdict tier 가 다르다** — 별도
> method 로 유지한다.

---

## purpose

서명 직전 상대방(EIP-2612 permit 의 `spender`, ERC-20 transfer 의 `recipient`, swap/bridge 의
counterparty)이 **어떤 risk typology 에 속하는지** — mixer · darknet_market · ransomware ·
terrorist_financing · stolen_funds · fraud_shop · high_risk_exchange · gambling · pep(정치적 주요인물) ·
high_risk_jurisdiction — 를 blockchain-analytics 공급자의 entity-typology 로 조회해 단일
`riskCategory: String` 로 투영한다. compliance/AML 류 정책은 이 한 라벨로 **warn**(또는 일부 라벨에서
deny)을 결정한다 — 예) "상대방이 mixer 면 경고", "sanctioned 엔티티면 차단". Dambi 은 시뮬레이션
없이 정적 분석만 하므로, "이 주소가 어떤 엔티티 유형으로 분류돼 있는가" 는 calldata 만으로 알 수 없고
외부 typology 피드 fetch 로만 얻는 사실이며, 이 method 가 그 fetch 를 캡슐화한다. Dambi 의
no-simulation 모델과 일관되게 이것은 **순수 fact lookup** 이지 트랜잭션 시뮬레이션이 아니다.

---

## interface

(상세 wire 규약은 SSOT = `POLICY_RPC_METHODS.md`. 단 작성 시점 §3c 에 본 method 미열거 — 아래는
구현이 반드시 지켜야 할 계약의 *제안*이며, 등록 시 §3c·method-catalog.json 에 그대로 반영.)

### params (input)

| param | type | selector (catalog 가 전달) | required | 의미 |
|---|---|---|---|---|
| `chain_id` | `Long` | `$.root.chain_id` | true | EIP-155 chain id. typology 피드는 chain 별로 다를 수 있어 캐시/조회 키의 일부. |
| `address` | `String` | `$.action.spender` (approval) / `$.action.recipient` (transfer) / `$.action.to` (일반) | true | 분류 대상 주소 (0x-prefixed, 소문자 정규화 권장). |

- 모든 catalog caller 는 `policy_rpc[].optional: true` (catalog enrichment 표준). 따라서 selector 가
  비면(예: decode 가 recipient 를 못 채움) **call 자체가 skip** 된다 — 이 method 핸들러는 호출조차
  받지 않는다. selector 철자의 authority 는 `/v1/rpc` planner 가 resolve 하는 **ActionView** JSON 이다
  (`POLICY_RPC_METHODS.md` §6). 구현자는 manifest 가 넘기는 selector 를 신뢰하고 자기 param 타입만
  맞추면 된다.

### result (output record)

```json
{ "riskCategory": "mixer", "confidence": 0.91, "source": "chainalysis" }
```

| field | type | required | 의미 |
|---|---|---|---|
| `riskCategory` | `String` | **yes** | 단일 canonical 라벨 (아래 enum). 분류 안 됨 = `"none"`, 판정 불가 = `"unknown"` (단, 진짜 미상은 field-omit 권장 — 아래 dormancy). 정책이 읽는 유일한 leaf. |
| `confidence` | `Decimal` | no | 공급자 분류 신뢰도(0–1). debug/reason 보강용. 정책 verdict 에는 미영향. |
| `source` | `String` | no | 어느 공급자가 분류했는가 (`chainalysis` \| `trm` \| `elliptic` \| `local-typology` …). 감사/디버깅용. |

**canonical 라벨 enum (제안)**: `mixer` \| `darknet_market` \| `ransomware` \| `terrorist_financing` \|
`stolen_funds` \| `fraud_shop` \| `high_risk_exchange` \| `gambling` \| `pep` \|
`high_risk_jurisdiction` \| `none`. 이 라벨 집합 자체는 **공급자별로 다르며**(Chainalysis entity
categories / TRM / Elliptic 각자의 typology), 위 목록은 Dambi 이 정책에서 비교할 **정규화된 상위
집합**이다 — 공급자 raw 라벨 → 이 정규 라벨로의 매핑 테이블이 NET-NEW 산출물이다. 정확한 공급자 라벨
집합은 **출처 미확인** (provider-specific, 공식 docs 에서 구현 시 확정).

### projection (record → scalar leaf — **mandatory**)

v2 `materialize_v2` 는 record ProjectionType 을 허용하지 않는다(`String|Long|Bool|Decimal|Set<String>`
스칼라만 — `POLICY_RPC_METHODS.md` §2). 따라서 manifest 가 leaf 스칼라로 투영한다:

```
$.result.riskCategory  -> String   (primary; 분류 비교 트리거; leaf = "riskCategory")
```

- `outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 강제). `outputs[].type`
  = `String`(capitalized); `custom_context` 철자는 lowercase Cedar (`"string"`).
- 정책은 `context.custom.riskCategory` (String) 한 개를 본다. 예:
  `context.custom has riskCategory && context.custom.riskCategory == "mixer"` → warn.
- `call_id = "<manifest_id>::<spec_id>"`. host fold 는 `results[call_id] = $.result` (unwrapped) 를
  engine 에 돌려준다 (`POLICY_RPC_METHODS.md` §1).

### method-catalog.json 등록 (구현 시 추가)

`schema/method-catalog.json` `methods` 에 아래 엔트리 추가 (현재 부재 = aspirational 의 원인):

```json
"address.category": {
  "name": "address.category",
  "description": "Risk-typology label of a counterparty (mixer/sanctioned/ransomware/… entity category).",
  "params": {
    "chain_id": { "type": "Long",   "required": true, "defaultSelector": "$.root.chain_id" },
    "address":  { "type": "String", "required": true, "description": "Counterparty address (spender/recipient/to)." }
  },
  "returns": { "kind": "scalar", "type": "String", "from": "$.result.riskCategory" },
  "origin": "bundled"
}
```

(record 전체를 반환하되 catalog `returns.kind` 는 투영 leaf 기준 `scalar/String`. `confidence`/`source`
는 result payload 에 같이 실려도 무방 — manifest 가 `riskCategory` 만 뽑는다.)

---

## data source(s)

핵심은 **address → entity-typology 라벨 조회**. 우선순위:

1. **외부 analytics typology API (1차, NET-NEW 플럼빙이지만 fetcher shape 재사용)**
   — Chainalysis / TRM Labs / Elliptic 의 address-screening / entity-categorization REST. HTTP GET +
   헤더 인증 + JSON pointer 추출 패턴은 **`RestJsonOracleFetcher`**
   (`crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs`) 와 1:1 동형이다:
   - env-keyed auth (`from_sync_config` 가 `std::env::var(env_var)` 로 빌드 시 주입, 빈 값이면 auth
     생략 → 키 없으면 외부 경로 자동 dormant. 이 패턴 그대로 채택; `rest_json.rs:31–39` 참조).
   - `base_url + path` GET → `resp.json()` → `body.pointer(json_pointer)` 로 라벨/category 필드 추출
     (timeout 은 `RestOracleConfig.timeout_sec` 차용).
   `address.category` 전용 fetcher 는 이 구조를 복제하되 `fetch_price→Decimal` 대신
   `fetch_category(chain, address) → {riskCategory, confidence?}` 시그니처로 만든다.

2. **로컬 shipped typology snapshot (2차, EXISTING-FETCHER-REUSABLE 플럼빙)**
   — 공개/스냅샷 가능한 일부 typology(예: 알려진 mixer 컨트랙트 주소, 공개 sanctions/OFAC SDN 주소
   목록)를 `(chain_id, address) → label` 정적 맵으로 서버에 동봉. 네트워크 0회, 항상 응답. registry-api
   가 이미 GCS 정적 리소스를 서빙하므로 **`RegistryFetcher`**
   (`crates/policy-server/sync/src/sources/fetchers/registry.rs`) 의 `Arc<RwLock<HashMap>>` TTL 캐시
   (`DEFAULT_CACHE_TTL = 24h`) + URL-build 패턴을 그대로 차용해 `typology/<chain>/<shard>` 형태로 받을
   수 있다. (단, PEP/jurisdiction/darknet 같은 비공개 typology 는 로컬 스냅샷으로 못 만든다 — 1차 API
   의존. 정직한 한계.)

**REUSE 요약**
- EXISTING-FETCHER-REUSABLE: `RegistryFetcher` (로컬/GCS typology snapshot + 24h TTL 캐시),
  `RestJsonOracleFetcher` (외부 API: env-auth + json_pointer 패턴).
- NET-NEW: (a) typology 전용 `DataSource` variant 또는 핸들러-로컬 클라이언트, (b) `/v1/rpc`
  `address.category` 핸들러, (c) **공급자 raw 라벨 → 정규 라벨 enum 매핑 테이블**, (d) PEP/jurisdiction
  attribution 데이터 자체(공급자 의존). 기존 fetcher 는 OracleFeed/RegistryApi `DataSource` 만 받으므로
  새 source 종류 또는 핸들러-로컬 클라이언트가 필요하다.

> 주의: `crates/.../sync/.../fetchers` 는 **decode-time `live_inputs` enrichment** 레이어이지
> `/v1/rpc` method registry 가 아니다 (`POLICY_RPC_METHODS.md` §5). 코드 패턴은 빌딩블록으로
> 재사용하되, 배선은 새 `/v1/rpc` 디스패처(method 키 → 핸들러)다.

---

## derivation algorithm

입력: `(chain_id: Long, address: String)`. 출력: `{riskCategory, confidence?, source?}` 또는 **field-omit**.

1. **정규화**: `address` 를 lowercase + EIP-55 무시(비교는 lowercase hex). 길이/0x prefix 검증. 형식
   불량 → 에러로 취급(아래 fallback: field-omit, default 주입 금지).
2. **로컬 typology snapshot 조회 (cache-first)**: `(chain_id, address)` 키로 shipped snapshot 조회.
   **hit → `{riskCategory:<entry.label>, source:"local-typology"}` 즉시 반환** (네트워크 0). (sanctions /
   알려진 mixer 처럼 공개 스냅샷이 있는 라벨만 여기서 잡힘.)
3. **(선택) 외부 API 조회**: 로컬 miss 이고 `<PROVIDER>_API_KEY` 가 설정돼 있으면 typology API GET.
   응답에서 entity category 추출 → **공급자 raw 라벨 → 정규 enum 매핑** 적용.
   - 한 주소가 **여러 category** 로 분류되면 canonical 단일 라벨 1개를 선택해야 한다(result 가 단일
     String). **우선순위 규칙을 명시**: 예) `sanctioned/terrorist_financing > ransomware/stolen_funds >
     darknet_market/fraud_shop > mixer > high_risk_exchange > gambling > pep > high_risk_jurisdiction`.
     (가장 심각한 라벨 우선. 이 우선순위는 휴리스틱 — 정직히 문서화하고 reason 에 노출.)
   - score/confidence 기반 피드면 **임계치 정책을 명시**(예) `confidence >= 0.7 인 category 만 채택`).
     임계치 미달이거나 category 없음 → 다음 단계.
4. **clean 판정 vs 불명 구분 (중요)**:
   - 공급자가 "어떤 risk typology 에도 속하지 않음(clean)" 이라고 **명시적으로** 응답 →
     `{riskCategory:"none"}` 반환 가능 (정책 inert, warn 안 됨).
   - 공급자 응답 불가/키 없음/타임아웃/형식불량/매핑 누락 → **`riskCategory` 를 채우지 말고 field-omit**
     (3-state: 분류됨 / none / unknown). unknown 을 `"none"` 으로 뭉개면 "조회 실패 = 깨끗함"으로
     오판하게 되므로 금지. (optional:true 라 omit 든 "none" 이든 verdict-flip 은 안 일어남 — 둘 다
     dormancy-safe 이지만, 의미 보존 위해 unknown 은 반드시 omit.)

**heuristic limit (정직한 한계)**
- **coverage 와 라벨 taxonomy 는 공급자 정의**다. 같은 주소가 공급자마다 다른 라벨/무라벨일 수 있다.
  Dambi 의 정규 enum 은 근사 매핑이며, 매핑 누락 = 그 정책 inert. 과장 금지: **매핑·공급자
  커버리지가 곧 이 method 의 커버리지**다.
- **PEP / high_risk_jurisdiction attribution 은 sparse** 하다 — 주소-수준 PEP 귀속은 공급자 데이터가
  희박하고 false-negative 가 많다. 이 두 라벨은 신뢰도 낮음을 reason 에 명시하고 **warn-tier**로만 쓰는
  것이 적절하다(deny 비권장).
- **post-hoc**: typology 등재는 사후다 — 새 mixer/darknet 주소는 등재 전까지 miss(false-negative).
  "알려진(known)" 한정임을 reason 에 과장 없이 표기.
- **false-positive 비용**: 과대 분류(정상 거래소를 high_risk_exchange 로)는 사용자의 정당한 거래를
  warn/deny 로 막는다. 보수적 임계치 + 대부분 warn-tier 로 비용을 낮춘다. sanctioned 등 일부 라벨만
  deny 로 승격하는 것은 정책 작성자 재량(이 method 는 라벨만 제공, tier 는 정책이 결정).

---

## on-chain calls

**none (off-chain / data-API).** entity typology 는 on-chain view 로 얻을 수 없다(컨트랙트가 "나는
mixer 다" 라고 노출하지 않음). 전부 로컬 typology snapshot 또는 off-chain analytics API fetch 다.
`chain_id` 는 RPC 대상이 아니라 **typology 파티션/캐시 키**로만 쓰인다.

---

## caching / ttl

| 항목 | 값 |
|---|---|
| cache key | `(chain_id, address_lowercase)` |
| 로컬 snapshot | 서버 부팅 시 메모리 로드(또는 GCS 스냅샷 24h refresh, `RegistryFetcher::DEFAULT_CACHE_TTL = 24h` 차용). 조회 O(1) 해시 — 사실상 0ms. |
| 외부 API 결과 | in-memory TTL 캐시. **positive(분류된 라벨) TTL 길게(≥1h)** — typology 는 비교적 안정적. negative("none") TTL 짧게(예 15–60m) — 새 등재 반영. `RegistryFetcher` 의 `Arc<RwLock<HashMap>>` + `inserted_at.elapsed() < ttl` 패턴 재사용. (구체 TTL 수치는 repo 내 명시 상수 없는 권장값 — **출처 미확인**.) |
| 위치 | `/v1/rpc` 디스패처 프로세스 내부(서버). 익스텐션은 캐시 안 함. |

**HARD_TIMEOUT_MS = 8000 예산 적합성** (orchestrator)
- 로컬 snapshot hit 경로는 네트워크 0 → 항상 budget 내.
- 외부 API 경로는 **per-call 타임아웃을 짧게**(예 1.5–2s, `RestJsonOracleFetcher` 의
  `reqwest ... .timeout(...)` 차용) 두고, 한 batch 의 다른 enrichment call 들과 합산해도 8s 안에 들도록.
  타임아웃 초과 → 에러 → field-omit(dormancy). 외부 API 가 느린 게 **정책을 막아선 안 됨**. 같은 주소를
  여러 compliance 정책이 한 action 에서 동시 조회해도 cache 가 1회로 흡수한다.

---

## failure & fallback (DORMANCY CONTRACT)

에러/미상/키 부재/타임아웃/매핑 누락 시 **`riskCategory` 필드를 절대 verdict-flip 가능한 값으로 채우지
않는다 (NO field emitted)**. 연쇄:

```
handler 가 riskCategory 를 emit 안 함
  → host fold: results[call_id] 에 riskCategory 없음 (또는 ok:false → 해당 output absent)
  → engine: context.custom 에 riskCategory(투영된 leaf) 부재
  → 정책 guard `context.custom has riskCategory` = false
  → when-조건 short-circuit → warn/forbid 미발화
  → 정책 INERT (pass) — 거짓 verdict 절대 없음
```

- **default 주입 금지**: `riskCategory` 를 `"none"` 으로도 임의 대입하지 않는다. `"none"` 대입은
  "조회 실패 = 깨끗함" 오판(실제 mixer/sanctioned 를 통과). 진짜 미상은 반드시 **omit**. (반대로 임의
  심각 라벨 대입은 무차별 warn/deny → false-positive. 둘 다 금지.)
- **clean ↔ unknown 구분**: 공급자가 명시적 clean 이면 `riskCategory:"none"` emit 가능(정책 inert,
  대부분 정책이 `== "none"` 을 트리거 안 함). 실패/미상은 omit. 둘 다 verdict-flip 은 아니지만 의미
  보존 위해 구분.
- **optional:true 의 효과**: 모든 catalog caller 가 `policy_rpc[].optional:true`. 따라서 이 call 의
  실패는 **batch 전체를 hard-fail 시키지 않고** 그 정책만 dormant 로 두며 → verdict 는 `pass` 로
  degrade(거짓 block 아님). (대조: `required:true` enrichment 실패는 `__system__` deny.)
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. fail-closed(deny) 방향으로
  흐르지 않는다 — 이는 enrichment 일반 계약이며, deny-closed 인 HyperLiquid venue 경로와 무관하다(이
  method 는 compliance enrichment 일 뿐 venue 차단 로직이 아니다).

---

## auth / cost / rate-limit

- **API key (env)**: 외부 analytics API 사용 시 `<PROVIDER>_API_KEY` 환경변수
  (예 `CHAINALYSIS_API_KEY`, `TRM_API_KEY`, `ELLIPTIC_API_KEY`) — `RestJsonOracleFetcher::from_sync_config`
  (`rest_json.rs:31–39`), `etherscan.rs::ETHERSCAN_API_KEY` 와 동일 컨벤션. **키가 비면 auth 생략 →
  외부 경로 자동 비활성 → 로컬 snapshot 만으로 동작**(키 없는 배포는 그 자체로 dormancy-safe; snapshot
  도 없으면 해당 정책 영구 inert).
- **per-call cost**: 로컬 snapshot 경로 = 0 (CPU 만). 외부 API 경로 = HTTP 1회 + 공급자 quota 1 unit.
  캐시 hit 은 비용 0. Chainalysis/TRM/Elliptic 는 **유료 enterprise 계약** 기반이라 quota·단가가 크다
  (구체 수치 **출처 미확인** — 공급자 약관 확인). 캐시 긴 TTL 로 비용/quota 압력을 흡수.
- **rate-limit**: 공급자별 quota 존재(**출처 미확인** — 구현자가 공급자 약관 확인). 캐시가 흡수: 반복
  동일 주소는 TTL 동안 1회만 외부 호출. rate-limit 초과 응답(429) → 에러 → field-omit(dormancy), batch
  비차단.

---

## activation

이 method 가 구현·등록되면 **~11개 compliance/risk-category catalog 정책**이 dormant → live 로 전환된다
(라벨별 1정책 근사: mixer / darknet_market / ransomware / terrorist_financing / stolen_funds /
fraud_shop / high_risk_exchange / gambling / pep / high_risk_jurisdiction + sanctioned-deny 류).

| 정책 (라벨) | 도메인/액션 | severity (제안) | guard field |
|---|---|---|---|
| `counterparty-mixer` | approval/transfer | warn | `context.custom.riskCategory == "mixer"` |
| `counterparty-darknet-market` | approval/transfer | warn | `... == "darknet_market"` |
| `counterparty-ransomware` | approval/transfer | warn/deny | `... == "ransomware"` |
| `counterparty-terrorist-financing` | approval/transfer | deny | `... == "terrorist_financing"` |
| `counterparty-stolen-funds` | approval/transfer | warn | `... == "stolen_funds"` |
| `counterparty-fraud-shop` | approval/transfer | warn | `... == "fraud_shop"` |
| `counterparty-high-risk-exchange` | approval/transfer | warn | `... == "high_risk_exchange"` |
| `counterparty-gambling` | approval/transfer | warn | `... == "gambling"` |
| `counterparty-pep` | approval/transfer | warn | `... == "pep"` |
| `counterparty-high-risk-jurisdiction` | approval/transfer | warn | `... == "high_risk_jurisdiction"` |

> 위 정책 id/severity 는 **제안**이다 — 작성 시점 55-policy catalog 에 이 risk-category 군이 아직
> 명시 등재돼 있지 않으면(미확인), 이 method 등록과 함께 정책도 같이 추가돼야 한다. 정확한 catalog
> 매핑은 `POLICY_RPC_METHODS.md` §4 activation map 의 SSOT 가 갱신될 때 그쪽을 따른다. severity tier
> (warn vs deny)는 정책 작성자 재량 — 이 method 는 **라벨만 제공**하고 tier 는 결정하지 않는다.
> PEP/jurisdiction 은 attribution sparse 로 **warn-tier 권장**.

---

## primary-source references

- **Chainalysis entity categories** — address/entity risk typology(공급자 정의 카테고리 집합). 정확한
  라벨 enum · 엔드포인트 · 인증 헤더 · 응답 스키마 · quota 는 **출처 미확인** (공식 docs 에서 구현 시
  확정). https://www.chainalysis.com/ (entity categorization / KYT API)
- **TRM Labs / Elliptic** — 대체 entity-typology 공급자 후보. 라벨 집합·계약·quota **출처 미확인**.
  https://www.trmlabs.com/ · https://www.elliptic.co/
- **FATF risk typologies** — mixer / VASP / high-risk jurisdiction 등 AML 위험 유형의 표준단체 정의
  (라벨의 규제적 근거). 정확한 typology 목록은 FATF 권고/가이던스 문서 기준 — 구현 시 확인.
  https://www.fatf-gafi.org/ (출처 미확인 for 주소-수준 attribution — FATF 는 분류 *기준*을 정의하지
  개별 주소를 분류하지 않음).
- **EIP-155** — chain id (param `chain_id` 의 정의). https://eips.ethereum.org/EIPS/eip-155
- **EIP-2612 / EIP-20** — permit `spender` / transfer `recipient` (분류 대상 주소가 나오는 surface).
  https://eips.ethereum.org/EIPS/eip-2612 · https://eips.ethereum.org/EIPS/eip-20
- **In-repo 패턴 (1차, 코드)**:
  - `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs` (`RestJsonOracleFetcher`,
    `:31–39` env-auth) — env-keyed auth + json_pointer fetch 패턴.
  - `crates/policy-server/sync/src/sources/fetchers/registry.rs` (`RegistryFetcher`,
    `DEFAULT_CACHE_TTL = 24h`) — TTL 캐시 + URL-build 패턴(로컬/GCS snapshot).
  - `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1(fold) · §2(record→scalar
    projection) · §3(method 표; **본 method 미열거 = aspirational 근거**) · §5(dispatcher 부재 현황) ·
    §6(selector 규약) — wire 계약 SSOT.
  - `schema/method-catalog.json` — 등록 스키마 형태 (현재 `address.category` 부재 = aspirational 근거).
  - 자매 spec `address.reputation.md` — 이진 flag(deny) vs 본 method 의 typology 라벨(warn) 구분 참조.

> Dambi 의 risk-category 정책 근거(compliance/AML 위협 진술)는 규제·공급자 문서 기반이며, 개별
> 주소의 라벨 정확도·커버리지·갱신주기는 **출처 미확인**(공급자별 상이) — 인용 시 공급자 공식 docs 및
> 라벨 정의를 별도 확인할 것.
