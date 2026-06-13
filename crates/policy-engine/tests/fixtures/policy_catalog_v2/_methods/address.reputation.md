# method: address.reputation

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> 이 문서는 **interface 재진술이 아니라 구현 지침**이다. wire shape 와 `$.`-selector 는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §`address.reputation` 가 SSOT 이고,
> 여기서는 *어떻게* 그 결과를 만들어내는지(데이터 소스 · 알고리즘 · 캐싱 · 실패 처리)를 정한다.
> 모든 사실 진술은 1차 출처 기반이며, 미확인은 "출처 미확인" 으로 명시한다.

---

## purpose

서명 직전 대상 주소(EIP-2612 permit 의 `spender`, ERC-20 transfer 의 `recipient`)가 **알려진
scam / drainer / phishing 주소인지** 를 denylist 피드로 조회해 단일 `flagged: Bool` 로 투영한다.
off-chain `permit` 서명은 on-chain tx 가 없어 wallet 경고가 약하고, drainer 가 victim 으로부터
무제한 allowance 를 빼내는 1순위 phishing surface 다 (SlowMist / ScamSniffer 보고). 정책은 이
한 비트로 **deny** 를 결정한다 — `flagged == true` 면 서명 자체를 차단한다. Dambi 은 시뮬레이션
없이 정적 분석만 하므로, "이 주소가 악성으로 등록돼 있는가" 는 외부 reputation 피드 fetch 로만
얻을 수 있는 사실이며, 이 method 가 그 fetch 를 캡슐화한다.

---

## interface

(상세 wire 규약은 `POLICY_RPC_METHODS.md` §`address.reputation` — 여기서는 구현이 반드시
지켜야 할 계약만 재진술)

### params (input)

| param | type | selector (catalog 가 전달) | required | 의미 |
|---|---|---|---|---|
| `chain_id` | `Long` | `$.root.chain_id` | true | EIP-155 chain id. denylist 는 chain 별로 다를 수 있으므로 키의 일부. |
| `address` | `String` | `$.action.spender` (approval) / `$.action.recipient` (transfer) | true | 평가 대상 주소 (0x-prefixed, 소문자 정규화 권장). |

- 두 catalog 정책 모두 `policy_rpc[].optional: true`. 따라서 selector 가 비면(예: decode 가
  spender 를 못 채움) **call 자체가 skip** 된다 — 이 method 핸들러는 호출조차 받지 않는다.

### result (output record)

```json
{ "flagged": true, "label": "drainer", "source": "blockaid" }
```

| field | type | required | 의미 |
|---|---|---|---|
| `flagged` | `Bool` | **yes** | denylist 에 등재돼 있으면 `true`. 등재 안 됨/판정 불가의 기본은 `false` 가 **아니라** field-omit (아래 dormancy 참조). |
| `label` | `String` | no | 분류 라벨 (`drainer` \| `phishing` \| `scam` \| `sanctioned` \| 피드 고유 문자열). UI reason 보강용. |
| `source` | `String` | no | 어느 피드가 flag 했는가 (`blockaid` \| `scamsniffer` \| `local-denylist` …). 감사/디버깅용. |

### projection (record → scalar leaf — **mandatory**)

v2 `materialize_v2` 는 record ProjectionType 을 허용하지 않는다(`String|Long|Bool|Decimal|Set<String>`
스칼라만). 따라서 manifest 가 leaf 스칼라로 투영한다:

```
$.result.flagged  -> Bool     (primary; deny 트리거)
$.result.label    -> String   (선택; reason 보강 — 별도 output 으로 투영 가능, 현 catalog 미사용)
```

- catalog wiring (참고, 변경 금지):
  - `permit-unknown-spender::spender-rep` → output `spenderFlagged: Bool` ← `$.result.flagged`
  - `transfer-recipient-reputation::recipient-rep` → output `recipientFlagged: Bool` ← `$.result.flagged`
- `call_id = "<manifest_id>::<spec_id>"` → `permit-unknown-spender::spender-rep`,
  `transfer-recipient-reputation::recipient-rep`. host fold 는 `results[call_id] = $.result`
  (unwrapped) 를 engine 에 돌려준다.

### method-catalog.json 등록 (구현 시 추가)

`schema/method-catalog.json` `methods` 에 아래 엔트리 추가 (현재 부재 = aspirational 의 원인):

```json
"address.reputation": {
  "name": "address.reputation",
  "description": "Is an address a known scam / drainer / phishing / sanctioned address (denylist lookup)?",
  "params": {
    "chain_id": { "type": "Long",   "required": true, "defaultSelector": "$.root.chain_id" },
    "address":  { "type": "String", "required": true, "description": "Address under review (spender/recipient)." }
  },
  "returns": { "kind": "scalar", "type": "Bool", "from": "$.result.flagged" },
  "origin": "bundled"
}
```

(record 전체를 반환하되 catalog `returns.kind` 는 투영 leaf 기준 `scalar/Bool`. `label`/`source` 는
result payload 에 같이 실려도 무방하다 — manifest 가 `flagged` 만 뽑는다.)

---

## data source(s)

핵심은 **address → {flagged, label} denylist 조회**. 우선순위:

1. **로컬 shipped denylist (1차, NET-NEW 데이터지만 EXISTING-FETCHER-REUSABLE 플럼빙)**
   — `(chain_id, address) → label` 정적 맵 / bloom filter 를 서버에 동봉. 네트워크 0회,
   HARD_TIMEOUT 무관, 항상 응답. registry-api 가 이미 GCS 정적 리소스를 서빙하므로
   `RegistryFetcher` (`crates/policy-server/sync/src/sources/fetchers/registry.rs`) 의
   24h TTL 캐시 + `build_url` 패턴을 그대로 차용해 `denylist/<chain>/<shard>` 형태로 받을 수 있다.
   denylist 는 ScamSniffer 등의 **공개 오픈소스 피드**를 빌드 시 스냅샷한다 (아래 출처).

2. **외부 reputation API (2차, NET-NEW 플럼빙이지만 fetcher shape 재사용)**
   — Blockaid / GoPlus / Chainalysis-style address-screening REST. HTTP GET + 헤더 인증 +
   JSON pointer 추출 패턴은 **`RestJsonOracleFetcher`**
   (`crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs`) 와 1:1 동형이다:
   - env-keyed auth (`from_sync_config` 가 `std::env::var(env_var)` 로 빌드 시 주입, 빈 값이면
     auth 생략 → 키 없으면 자동 dormant. 이 패턴을 그대로 채택).
   - `base_url + path` GET → `resp.json()` → `body.pointer(json_pointer)` 추출.
   `address.reputation` 전용 fetcher 는 이 구조를 복제하되 `fetch_price→Decimal` 대신
   `fetch_reputation(chain, address) → {flagged, label}` 시그니처로 만든다.

**REUSE 요약**
- EXISTING-FETCHER-REUSABLE: `RegistryFetcher` (로컬/GCS denylist + TTL 캐시),
  `RestJsonOracleFetcher` (외부 API: env-auth + json_pointer 패턴).
- NET-NEW: reputation 전용 `DataSource` variant + `/v1/rpc` `address.reputation` 핸들러 +
  denylist 스냅샷 데이터 자체. (기존 fetcher 들은 OracleFeed/RegistryApi `DataSource` 만 받으므로
  새 source 종류 또는 핸들러-로컬 클라이언트가 필요.)

> 주의: `crates/.../sync/.../fetchers` 는 **decode-time `live_inputs` enrichment** 레이어이지
> `/v1/rpc` method registry 가 아니다 (POLICY_RPC_METHODS.md §5). 코드 패턴은 빌딩블록으로
> 재사용하되, 배선은 새 `/v1/rpc` 디스패처(method 키 → 핸들러)다.

---

## derivation algorithm

입력: `(chain_id: Long, address: String)`. 출력: `{flagged, label?, source?}` 또는 **field-omit**.

1. **정규화**: `address` 를 lowercase + EIP-55 무시(비교는 lowercase hex). 길이/0x prefix 검증.
   형식 불량 → 에러로 취급(아래 fallback: field-omit, default 주입 금지).
2. **로컬 denylist 조회 (cache-first)**: `(chain_id, address)` 키로 shipped denylist / bloom
   조회. **hit → `{flagged:true, label:<entry.label>, source:"local-denylist"}` 즉시 반환** (네트워크 0).
3. **(선택) 외부 API 조회**: 로컬 miss 이고 `<PROVIDER>_API_KEY` 가 설정돼 있으면 reputation API
   GET. 응답에서 `flagged`(또는 risk-score ≥ 임계치)와 `label` 추출.
   - score-기반 피드면 **임계치 정책을 명시**: 예) `riskScore >= 80 ⇒ flagged:true`. 임계치는
     상수로 두고 reason 에 노출. (어떤 점수에서 차단할지는 휴리스틱 — 정직히 문서화.)
   - API hit(flagged) → `{flagged:true, label, source:"<provider>"}`.
4. **clean 판정 vs 불명 구분 (중요)**:
   - 피드가 "명시적으로 clean" 이라고 응답 → `{flagged:false}` 반환 가능 (정책은 inert, deny 안 됨).
   - 피드 자체가 응답 불가/키 없음/타임아웃/형식불량 → **`flagged` 를 채우지 말고 field-omit**
     (3-state: flagged / clean / unknown). unknown 을 `false` 로 뭉개면 "조회 실패 = 안전"으로
     오판하게 되므로 금지. (단, optional:true 라 omit 든 false 든 deny 는 안 일어남 — 둘 다
     dormancy-safe 이지만, 의미 보존 위해 unknown 은 omit 권장.)

**heuristic limit (정직한 한계)**
- denylist 는 **사후(post-hoc)** 다 — 새로 만든 drainer 주소는 등재 전까지 miss. false-negative
  존재. Dambi 은 이 한계를 reason 에 과장 없이 표기해야 한다("known-malicious" — *알려진* 한정).
- score-임계치 차단은 피드 캘리브레이션에 의존하는 휴리스틱이며, false-positive(정상 주소
  과대 flag) 시 사용자가 정당한 transfer 를 못 함 → deny severity 라 비용이 크다. 임계치는
  보수적으로(높은 score 만) 설정.
- chain 별 커버리지 불균형: 메이저 EVM 외 chain 은 denylist 가 비어 대부분 unknown 일 수 있음.

---

## on-chain calls

**none (off-chain / data-API).** reputation 은 on-chain view 로 얻을 수 없다(컨트랙트가 "나는
drainer 다" 라고 노출하지 않음). 전부 로컬 denylist 또는 off-chain reputation API fetch 다.
`chain_id` 는 RPC 대상이 아니라 **denylist 파티션 키**로만 쓰인다.

---

## caching / ttl

| 항목 | 값 |
|---|---|
| cache key | `(chain_id, address_lowercase)` |
| 로컬 denylist | 서버 부팅 시 메모리 로드(또는 GCS 스냅샷 24h refresh, `RegistryFetcher` 의 `DEFAULT_CACHE_TTL = 24h` 차용). 조회는 O(1) 해시/bloom — 사실상 0ms. |
| 외부 API 결과 | in-memory TTL 캐시. **positive(flagged) TTL 길게(≥1h)**, negative(clean) TTL 짧게(예 5–15m) — 새 등재를 빨리 반영하기 위함. `RegistryFetcher` 의 `Arc<RwLock<HashMap>>` + `inserted_at.elapsed() < ttl` 패턴 재사용. |
| 위치 | `/v1/rpc` 디스패처 프로세스 내부(서버). 익스텐션은 캐시 안 함. |

**HARD_TIMEOUT_MS = 8000 예산 적합성**
- 로컬 denylist hit 경로는 네트워크 0 → 항상 budget 내.
- 외부 API 경로는 **per-call 타임아웃을 짧게**(예 1.5–2s, `RestJsonOracleFetcher` 의
  `reqwest ... .timeout(...)` 차용) 두고, 한 batch 에 다른 enrichment call 들과 합산해도 8s 안에
  들도록. 타임아웃 초과 → 에러 → field-omit(dormancy). 외부 API 가 느린 게 **정책을 막아선 안 됨**.

---

## failure & fallback (DORMANCY CONTRACT)

에러/미상/키 부재/타임아웃 시 **`flagged` 필드를 절대 채우지 않는다 (NO field emitted)**. 연쇄:

```
handler 가 flagged 를 emit 안 함
  → host fold: results[call_id] 에 flagged 없음 (또는 ok:false → 해당 output absent)
  → engine: context.custom 에 spenderFlagged / recipientFlagged 부재
  → 정책 guard `context.custom has spenderFlagged` (그리고 `... has recipientFlagged`) = false
  → when-조건 short-circuit → forbid 미발화
  → 정책 INERT (pass) — 거짓 verdict 절대 없음
```

- **default 주입 금지**: `flagged` 를 `false` 로도 `true` 로도 임의 대입하지 않는다. `false`
  대입은 "조회 실패 = 안전" 오판(실제 악성을 통과), `true` 대입은 무차별 차단(false-positive
  deny). unknown 은 반드시 **omit**.
- **optional:true 의 효과**: 두 catalog 정책 모두 `policy_rpc[].optional:true`. 따라서 이 call 의
  실패는 **batch 전체를 hard-fail 시키지 않고** 그 정책만 dormant 로 두며 → verdict 는 `pass` 로
  degrade(거짓 block 아님). (대조: `required:true` enrichment 실패는 `__system__` deny.)
- **clean ↔ unknown 구분**: 피드가 명시적 clean 이면 `flagged:false` emit 가능(정책 inert).
  실패/미상은 omit. 둘 다 deny 는 아니지만 의미 보존 위해 구분.

---

## auth / cost / rate-limit

- **API key (env)**: 외부 reputation API 사용 시 `<PROVIDER>_API_KEY` 환경변수
  (예 `BLOCKAID_API_KEY`, `GOPLUS_API_KEY`) — `RestJsonOracleFetcher::from_sync_config`,
  `discovery/coingecko.rs::std::env::var("COINGECKO_API_KEY")`, `etherscan.rs::ETHERSCAN_API_KEY`
  와 동일 컨벤션. **키가 비면 auth 생략 → 외부 경로 자동 비활성 → 로컬 denylist 만으로 동작**
  (키 없는 배포는 그 자체로 dormancy-safe).
- **per-call cost**: 로컬 denylist 경로 = 0 (CPU 만). 외부 API 경로 = HTTP 1회 + 공급자 quota
  1 unit. 캐시 hit 은 비용 0.
- **rate-limit**: 공급자별 quota 존재(출처 미확인 — 구현자가 공급자 약관 확인). 캐시가 흡수:
  반복되는 동일 spender/recipient 는 TTL 동안 1회만 외부 호출. positive 결과 긴 TTL 로 hot
  drainer 주소의 재조회를 억제. rate-limit 초과 응답(429) → 에러 → field-omit(dormancy), batch
  비차단.

---

## activation

이 method 가 구현·등록되면 아래 catalog 정책이 dormant → live 로 전환된다:

| 정책 id | 도메인/액션 | severity | guard field | call_id |
|---|---|---|---|---|
| `permit-unknown-spender` | action/approval (`erc20_permit`) | deny | `context.custom.spenderFlagged == true` | `permit-unknown-spender::spender-rep` |
| `transfer-recipient-reputation` | action/transfer (`erc20_transfer`) | deny | `context.custom.recipientFlagged == true` | `transfer-recipient-reputation::recipient-rep` |

두 정책 모두 `flagged == true` 일 때만 `forbid` 발화. method 부재(현 상태) 에서는 field 가
안 채워져 영구 inert — 즉 **이 spec 구현이 두 deny 정책을 깨우는 유일한 트리거**다.

---

## primary-source references

- **EIP-2612** — `permit()` (ERC-20 gasless approval via signature). off-chain 서명이 곧
  allowance 위임이 되는 메커니즘의 정의. https://eips.ethereum.org/EIPS/eip-2612
- **EIP-712** — typed structured data signing (permit 서명 페이로드의 기반).
  https://eips.ethereum.org/EIPS/eip-712
- **EIP-155** — chain id (params `chain_id` 의 정의). https://eips.ethereum.org/EIPS/eip-155
- **ScamSniffer — scam-database (오픈소스 denylist 후보)** — address/domain blacklist 공개 레포.
  로컬 denylist 스냅샷 소스 후보. https://github.com/scamsniffer/scam-database
  (피드 정확도/커버리지/갱신주기는 레포 README 기준 — 정량 수치는 **출처 미확인**, 구현자 검증)
- **Blockaid / GoPlus address-screening API** — 외부 reputation API 후보. 정확한 엔드포인트 ·
  인증 헤더 · 응답 스키마 · quota 는 **출처 미확인** (각 공급자 공식 docs 에서 구현 시 확정).
- **In-repo 패턴 (1차, 코드)**:
  - `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs` — env-auth + json_pointer fetch 패턴.
  - `crates/policy-server/sync/src/sources/fetchers/registry.rs` — TTL 캐시(24h) + URL-build 패턴.
  - `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §`address.reputation`, §1(fold),
    §2(record→scalar projection), §5(dispatcher 부재 현황) — wire 계약 SSOT.
  - `schema/method-catalog.json` — 등록 스키마 형태 (현재 `address.reputation` 부재 = aspirational 근거).

> SlowMist / ScamSniffer permit-phishing 위협 진술(정책 .cedar 주석 인용)은 **2차 post-mortem**
> 범주이며 본 spec 의 사실 진술 근거로는 약함. drainer 손실 규모 등 정량치는 **출처 미확인** —
> 인용 시 공식 보고서 날짜+규모를 별도 확인할 것.
