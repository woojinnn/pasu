# method: address.sanctions

status: aspirational (NOT in method-catalog.json — register on implement)

> 이 문서는 **interface 재진술이 아니라 구현 지침**이다. wire shape 와 `$.`-selector 의 SSOT 는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` 이고, 여기서는 *어떻게* 그 결과를
> 만들어내는지(데이터 소스 · 알고리즘 · 캐싱 · 실패 처리)를 정한다. 모든 사실 진술은 1차 출처 기반이며,
> 검증 못 한 것은 "출처 미확인" 으로 명시한다 — 추측을 사실로 둔갑시키지 않는다.

---

## purpose

서명 직전 대상 주소(EIP-2612 permit 의 `spender`, ERC-20 transfer 의 `recipient`, swap/borrow 의
counterparty 등)가 **법적 제재 명단(OFAC SDN / EU / UN sanctions list)에 등재돼 있는지** 를
조회해 단일 `sanctioned: Bool` 로 투영한다.

`address.sanctions` 는 **`address.reputation` 와 명확히 구분**된다:

- `address.reputation` = **scam / drainer / phishing 휴리스틱** (사용자 *자산 보호* 목적, denylist 가
  사후·확률적, false-negative 허용).
- `address.sanctions` = **법적 컴플라이언스 list 멤버십** (제재 *준수* 목적). 출처가 정부 발행 공식
  명단(OFAC Specially Designated Nationals + Digital Currency Address list 등)이며, 멤버십은
  확률이 아니라 **공식 등재 여부**라는 이산 사실이다. 정책은 이 한 비트로 제재 대상과의 상호작용을
  **차단(deny)** 하거나 issuer-freeze 류 정책의 게이트로 쓴다.

Dambi 은 시뮬레이션 없이 정적 분석만 하므로 "이 주소가 제재 명단에 있는가" 는 calldata 만으로는
알 수 없고 — 외부/로컬 제재 명단 fetch 로만 얻는 사실이다. 이 method 가 그 fetch 를 캡슐화하고,
명단 **staleness(신선도)** 까지 별도 비트(`sanctionsFresh`)로 표면화해 "오래된 명단으로 통과시킴" 을
fail-on-stale 메타 정책이 막을 수 있게 한다.

---

## interface

(상세 wire 규약은 `POLICY_RPC_METHODS.md` — 여기서는 구현이 반드시 지켜야 할 계약만 재진술)

### params (input)

| param | type | selector (catalog 가 전달) | required | 의미 |
|---|---|---|---|---|
| `address` | `String` | `$.action.spender` (approval) / `$.action.recipient` (transfer) / action 별 counterparty | true | 평가 대상 주소 (0x-prefixed, 소문자 정규화 권장). |
| `chain_id` | `Long` | `$.root.chain_id` | no (optional) | EIP-155 chain id. OFAC Digital Currency Address list 는 자산/체인별로 등재될 수 있으므로 캐시·매칭 키의 일부. 부재 시 chain-agnostic 매칭으로 degrade. |

- catalog 정책은 `policy_rpc[].optional: true` (catalog enrichment 표준). selector 가 비면(예: decode 가
  spender 를 못 채움) **call 자체가 skip** 된다 — 이 method 핸들러는 호출조차 받지 않는다.
- `chain_id` 가 optional 인 이유: OFAC SDN 명단의 **digital currency address** 엔트리는 주소가
  특정 체인에 묶이지 않고 등재되는 경우가 많다(동일 주소가 여러 EVM 체인에 존재). 따라서 1차 매칭은
  주소 hex 자체이고, `chain_id` 는 체인-한정 엔트리·캐시 파티션에만 쓴다.

### result (output record)

```json
{ "sanctioned": true, "list": "OFAC-SDN", "sanctionsFresh": true }
```

| field | type | required | 의미 |
|---|---|---|---|
| `sanctioned` | `Bool` | **yes** | 제재 명단에 등재돼 있으면 `true`. 미등재/판정 불가의 기본은 `false` 가 **아니라** field-omit (아래 dormancy 참조). |
| `list` | `String` | no | 어느 명단에서 hit 했는가 (`OFAC-SDN` \| `OFAC-DCA` \| `EU` \| `UN` \| `local-snapshot` …). UI reason / 감사용. |
| `sanctionsFresh` | `Bool` | no | 사용한 명단 스냅샷이 **신선한가** (등재 데이터의 `as_of` 가 staleness 한도 내인가). fail-on-stale 메타 정책이 읽는 leaf. `false` = 명단이 오래됨(아래 staleness 알고리즘). |

> `sanctioned` 외 필드는 부가/메타다. v2 엔진은 record 를 통째로 받지 못하므로(아래 projection) 이
> 필드들은 manifest 가 leaf 로 따로 투영해야만 정책에 보인다 — 구현 1차 컷은 `sanctioned` 만 채워도
> 핵심 deny 정책은 동작한다. `sanctionsFresh` 는 fail-on-stale 메타 정책을 깨우려면 같이 채운다.

### projection (record → scalar leaf — **mandatory**)

v2 `materialize_v2` 는 record ProjectionType 을 허용하지 않는다 — scalar 만
(`String | Long | Bool | Decimal | Set<String>`). legacy record type(`UsdValuation`, `WindowStats`)은
제거됐다(`POLICY_RPC_METHODS.md` §2). 따라서 manifest 가 leaf 스칼라로 투영한다:

```
$.result.sanctioned     -> Bool     (primary; deny 트리거)
$.result.sanctionsFresh  -> Bool     (선택; fail-on-stale 메타 정책 — 별도 output 으로 투영)
$.result.list            -> String   (선택; reason 보강 — 현 catalog 미사용)
```

- `outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 가 강제).
  `outputs[].type` 은 capitalized(`Bool`), `custom_context` 철자는 lowercase Cedar(`"bool"`).
- `call_id = "<manifest_id>::<spec_id>"`. host fold 는 `results[call_id] = $.result` (unwrapped) 를
  engine 에 돌려준다 — manifest 가 `sanctioned`(및 `sanctionsFresh`) leaf 를 뽑는다.

### method-catalog.json 등록 (구현 시 추가)

`schema/method-catalog.json` `methods` 에 아래 엔트리 추가 (현재 부재 = aspirational 의 원인):

```json
"address.sanctions": {
  "name": "address.sanctions",
  "description": "Is an address on an official sanctions list (OFAC SDN / Digital Currency Address / EU / UN)? Legal-list membership, distinct from address.reputation scam-heuristic.",
  "params": {
    "address":  { "type": "String", "required": true,  "description": "Address under review (spender/recipient/counterparty)." },
    "chain_id": { "type": "Long",   "required": false, "defaultSelector": "$.root.chain_id" }
  },
  "returns": { "kind": "scalar", "type": "Bool", "from": "$.result.sanctioned" },
  "origin": "bundled"
}
```

(record 전체를 반환하되 catalog `returns.kind` 는 투영 leaf 기준 `scalar/Bool`. `list`/`sanctionsFresh`
는 result payload 에 같이 실려도 무방 — manifest 가 필요한 leaf 만 뽑는다.)

---

## data source(s)

핵심은 **address → {sanctioned, list, as_of} 공식 명단 조회**. 우선순위:

### 1. 로컬 동기화 sanctions set (1차) — NET-NEW 데이터 / EXISTING-FETCHER-REUSABLE 플럼빙

- **무엇**: OFAC Sanctions List Service 가 발행하는 **SDN list + Consolidated list 의 Digital Currency
  Address(DCA) 필드** 를 빌드/주기적으로 스냅샷해 `(address_lowercase) → {list, as_of}` 정적
  set / bloom filter 로 서버에 동봉. 네트워크 0회, HARD_TIMEOUT 무관, 항상 응답.
- **REUSE**: registry-api 가 이미 GCS 정적 리소스를 서빙하므로 `RegistryFetcher`
  (`crates/policy-server/sync/src/sources/fetchers/registry.rs`) 의 `Arc<RwLock<HashMap>>` TTL 캐시
  (`DEFAULT_CACHE_TTL = 24h`) + `build_url` 패턴을 그대로 차용해 `sanctions/<list>/<shard>` 형태로
  주기 스냅샷을 받는다. OFAC 는 명단을 정기 갱신하므로(매일 변경될 수 있음) GCS 스냅샷 refresh
  주기를 짧게(아래 caching) 둔다.
- **NET-NEW**: OFAC SDN XML/CSV(또는 SDN.XML 의 `DigitalCurrencyAddress` 필드) → `(address) → {list, as_of}`
  파서 + 빌드/주기 인제스트 잡. (OFAC 배포 포맷은 SDN.XML/SDN.CSV — 정확한 스키마는 아래 출처에서
  구현 시 확정.)

### 2. 외부 screening 제공자 API (2차) — NET-NEW 플럼빙 / fetcher shape 재사용

- **무엇**: Chainalysis / TRM Labs 등 **sanctions screening REST API**. (Chainalysis 는 무료
  on-chain sanctions oracle / screening API 를 공개한 바 있음 — 정확한 엔드포인트·인증·응답 스키마는
  **출처 미확인**, 구현 시 각 제공자 공식 docs 확정.)
- **REUSE**: HTTP GET + 헤더 인증 + JSON pointer 추출 패턴은 `RestJsonOracleFetcher`
  (`crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs`) 와 1:1 동형:
  - env-keyed auth (`from_sync_config` 가 `std::env::var(&a.env_var)` 로 빌드 시 주입; 빈 값이면
    auth header 생략 → **키 없으면 외부 경로 자동 비활성**. 이 패턴 그대로 채택).
  - `base_url + path` GET → `resp.json()` → `body.pointer(json_pointer)` 추출.
  - `reqwest ... .timeout(cfg.timeout_sec)` per-call 타임아웃.
  - `address.sanctions` 전용 fetcher 는 이 구조를 복제하되 `fetch_price→Decimal` 대신
    `fetch_sanctions(address[, chain]) → {sanctioned, list, as_of}` 시그니처로 만든다.

> 일부 제공자는 **on-chain sanctions oracle 컨트랙트**(`isSanctioned(address)` view)를 제공할 수도
> 있다. 그 경로를 쓴다면 on-chain read 가 되며 `OnchainViewFetcher`
> (`crates/policy-server/sync/src/sources/fetchers/onchain.rs`) 의 `eth_call` + selector 인코딩
> 패턴 재사용 — 단, 명단 자체는 여전히 off-chain 발행물이고 oracle 은 그 미러일 뿐이다. 기본 설계는
> 로컬 set(1차) 우선이며 on-chain oracle 은 선택지로만 기록한다.

**REUSE 요약**
- EXISTING-FETCHER-REUSABLE: `RegistryFetcher` (로컬/GCS sanctions 스냅샷 + TTL 캐시),
  `RestJsonOracleFetcher` (외부 screening API: env-auth + json_pointer + timeout 패턴),
  (선택) `OnchainViewFetcher` (on-chain sanctions oracle view 를 쓸 경우).
- NET-NEW: sanctions 전용 `DataSource` variant + `/v1/rpc` `address.sanctions` 핸들러 + OFAC SDN/DCA
  스냅샷·파서·인제스트 + staleness(`as_of`) 추적. (기존 fetcher 들은 OracleFeed/RegistryApi `DataSource`
  만 받으므로 새 source 종류 또는 핸들러-로컬 클라이언트가 필요.)

> 주의: `crates/.../sync/.../fetchers` 는 **decode-time `live_inputs` enrichment** 레이어이지
> `/v1/rpc` method registry 가 아니다 (`POLICY_RPC_METHODS.md` §5). 코드 패턴은 빌딩블록으로
> 재사용하되, 배선은 새 `/v1/rpc` 디스패처(method 키 → 핸들러)다.

---

## derivation algorithm

입력: `(address: String, chain_id?: Long)`. 출력: `{sanctioned, list?, sanctionsFresh?}` 또는 **field-omit**.

1. **정규화**: `address` lowercase + 0x prefix/길이(20-byte hex) 검증. EIP-55 체크섬은 무시(비교는
   lowercase hex). 형식 불량 → 에러 취급(아래 fallback: field-omit, default 주입 금지).
2. **로컬 sanctions set 조회 (cache-first)**: `address` (체인-한정 엔트리면 `(chain_id, address)`) 키로
   shipped/GCS 스냅샷 조회.
   - **hit → `{sanctioned: true, list: <entry.list>, sanctionsFresh: <staleness 계산>}` 반환** (네트워크 0).
3. **(선택) 외부 screening API 조회**: 로컬 miss 이고 `<PROVIDER>_API_KEY` 설정 시 screening API GET.
   응답에서 sanctioned 여부 + 명단명 추출.
   - hit → `{sanctioned: true, list: "<provider>", sanctionsFresh: <provider 응답의 as_of 기준>}`.
4. **sanctioned vs unknown 구분 (중요)**:
   - 명단이 **명시적으로 미등재(clean)** 로 답 → `{sanctioned: false, sanctionsFresh: <staleness>}` 반환
     가능 (정책 inert, deny 안 됨). `sanctionsFresh` 는 미등재 답에도 채워 staleness 메타 정책이 동작.
   - 명단 자체 응답 불가/키 없음/타임아웃/형식불량 → **`sanctioned` field-omit** (3-state:
     sanctioned / clean / unknown). unknown 을 `false` 로 뭉개면 "조회 실패 = 안전" 오판이므로 금지.
5. **staleness 계산 (`sanctionsFresh`)**:
   - 사용한 스냅샷/응답의 `as_of`(또는 GCS 스냅샷 `inserted_at`)를 `clock.now` 기준 경과시간으로
     평가. `elapsed <= FRESHNESS_MAX` → `sanctionsFresh: true`, 초과 → `false`.
   - `FRESHNESS_MAX` 는 상수로 두고 reason 에 노출. OFAC 는 명단을 빈번히(일 단위로) 갱신할 수 있으므로
     보수적으로 짧게 권장(예 24~48h) — 정확한 갱신 SLA 는 **출처 미확인**, 구현 시 OFAC 발행 주기로 확정.
   - `sanctionsFresh: false` 라도 `sanctioned` 는 가지고 있던 명단 기준으로 *값을 준다* — staleness
     차단은 **별도 fail-on-stale 메타 정책**(`sanctionsFresh == false ⇒ warn/fail`)이 결정하지, 이
     method 가 임의 default 로 막지 않는다.

**heuristic / 한계 (정직한 한계)**

- **명단 멤버십은 확률이 아니라 이산 사실**이지만, 그 사실의 **신선도**가 핵심 리스크다 — 막 등재된
  주소가 로컬 스냅샷에 반영되기 전이면 miss(false-negative). `sanctionsFresh` 가 이 staleness 를
  표면화하는 이유다.
- **주소-매칭의 한계**: OFAC DCA 엔트리는 주소 단위지만, 제재 *엔티티* 가 새로 만든 미등재 주소나
  mixer 경유 자금은 주소 매칭으로 못 잡는다(엔티티 클러스터링은 이 method 범위 밖 — screening
  제공자의 graph 분석 영역).
- **법적 판단 아님**: 이 비트는 "공식 명단에 해당 주소 hex 가 있는가" 의 *기술적* 조회다. 실제 제재
  적용 여부·관할·예외는 법적 판단이며 Dambi 의 verdict reason 은 이를 "명단 등재 사실" 로만
  과장 없이 표기해야 한다.
- **chain 별 커버리지 불균형**: 메이저 EVM 외 chain 의 DCA 커버리지는 명단 발행물에 의존 — 대부분
  unknown 일 수 있음.

---

## on-chain calls

**none (off-chain / 공식 명단 발행물).** 제재 명단 멤버십은 정부 발행 데이터이지 on-chain view 가
아니다(컨트랙트가 "나는 제재 대상" 이라 노출하지 않음). 기본 경로는 전부 로컬 sanctions set 또는
off-chain screening API fetch 다. `chain_id` 는 RPC 대상이 아니라 **명단 파티션/매칭 키**로만 쓰인다.

> 예외(선택지): 외부 제공자의 **on-chain sanctions oracle**(`isSanctioned(address)` view)을 쓰기로
> 하면 그때만 `eth_call` 1회가 생긴다 — `OnchainViewFetcher` 의 selector 인코딩 + `RpcRouter::eth_call`
> 패턴 재사용. 기본 설계는 oracle 비사용(로컬 set 우선)이며 RPC 0회다.

---

## caching / ttl

| 항목 | 값 |
|---|---|
| cache key | `address_lowercase` (체인-한정 엔트리면 `(chain_id, address_lowercase)`) |
| 로컬 sanctions set | 서버 부팅 시 메모리 로드 + GCS 스냅샷 주기 refresh. `RegistryFetcher` 의 24h TTL 캐시 패턴 차용하되, OFAC 갱신 빈도를 고려해 **스냅샷 refresh 주기는 짧게**(예 6~24h) — staleness 비용을 줄임. 조회는 O(1) 해시/bloom ≈ 0ms. |
| 외부 API 결과 | in-memory TTL 캐시. **positive(sanctioned) TTL 길게**(등재는 잘 안 풀림), negative(clean) TTL 짧게(예 5~15m) — 새 등재를 빨리 반영. `RegistryFetcher` 의 `inserted_at.elapsed() < ttl` 패턴 재사용. |
| `as_of` 보존 | 스냅샷/응답의 발행 시각을 캐시 엔트리에 같이 저장 → `sanctionsFresh` 계산에 사용. 캐시 hit 도 staleness 는 매번 `clock.now` 기준 재평가. |
| 위치 | `/v1/rpc` 디스패처 프로세스 내부(서버). 익스텐션은 캐시 안 함. |

**HARD_TIMEOUT_MS = 8000 예산 적합성**
- 로컬 set hit 경로는 네트워크 0 → 항상 budget 내.
- 외부 API 경로는 **per-call 타임아웃 짧게**(예 1.5~2s, `RestJsonOracleFetcher` 의 `.timeout(...)`
  차용) + 한 batch 의 다른 enrichment 와 합산해도 8s 내. 타임아웃 초과 → 에러 → `sanctioned`
  field-omit(dormancy). 외부 API 가 느린 게 **정책을 막아선 안 됨**.

이 TTL/refresh 수치들은 본 repo 코드에 명시 상수가 없는 **권장값**이다 — OFAC 갱신 SLA·제공자 quota
기준으로 구현 시 확정. **출처 미확인**.

---

## failure & fallback (DORMANCY CONTRACT)

에러/미상/키 부재/타임아웃 시 **`sanctioned` 필드를 절대 채우지 않는다 (NO field emitted)**. 연쇄:

```
handler 가 sanctioned 를 emit 안 함
  → host fold: results[call_id] 에 sanctioned 없음 (또는 ok:false → 해당 output absent)
  → engine: context.custom 에 sanctioned leaf 부재
  → 정책 guard `context.custom has sanctioned` = false
  → when-조건 short-circuit → forbid/warn 미발화
  → 정책 INERT (pass) — 거짓 verdict 절대 없음
```

- **default 주입 금지**: `sanctioned` 를 `false` 로도 `true` 로도 임의 대입하지 않는다. `false` 대입은
  "조회 실패 = 안전" 오판(실제 제재 대상 통과), `true` 대입은 무차별 차단(false-positive deny).
  unknown 은 반드시 **omit** — verdict 를 뒤집는 default 금지.
- **`sanctionsFresh` 도 동일**: staleness 판정 불가면 `sanctionsFresh` 도 omit → fail-on-stale 메타
  정책의 `has sanctionsFresh` guard = false → 그 정책도 inert. staleness 미상을 `true`(신선) 로
  대입해 stale 차단을 우회시키지 않는다.
- **optional:true 의 효과**: catalog 정책은 `policy_rpc[].optional:true`. 이 call 의 실패는 **batch
  전체를 hard-fail 시키지 않고** 그 정책만 dormant 로 두며 → verdict 는 `pass` 로 degrade(거짓 block
  아님). (대조: `required:true` enrichment 실패는 `__system__` deny.)
- **clean ↔ unknown 구분**: 명단이 명시적 미등재면 `sanctioned:false` emit 가능(정책 inert).
  실패/미상은 omit. 둘 다 deny 는 아니지만 의미 보존 위해 구분.
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. (이는 enrichment 일반
  계약이고 deny-closed 인 HyperLiquid venue 경로와 무관 — 이 method 는 컴플라이언스 enrichment 일 뿐
  venue 차단 로직이 아니다.)

---

## auth / cost / rate-limit

- **로컬 set**: 인증 불필요, 비용 0(CPU 만), rate-limit 없음. OFAC 명단은 공개 발행물이므로 스냅샷
  인제스트 자체에 API key 불요(다운로드만).
- **API key (env)**: 외부 screening 제공자 사용 시 `<PROVIDER>_API_KEY` 환경변수
  (예 `CHAINALYSIS_API_KEY`, `TRM_API_KEY`) — `RestJsonOracleFetcher::from_sync_config`,
  `discovery/coingecko.rs::std::env::var("COINGECKO_API_KEY")`, `etherscan.rs::ETHERSCAN_API_KEY`
  와 동일 컨벤션. **키가 비면 auth 생략 → 외부 경로 자동 비활성 → 로컬 sanctions set 만으로 동작**
  (키 없는 배포는 그 자체로 dormancy-safe).
- **per-call cost**: 로컬 set 경로 = 0. 외부 API 경로 = HTTP 1회 + 제공자 quota 1 unit. 캐시 hit = 0.
- **rate-limit**: 제공자별 quota 존재(**출처 미확인** — 구현자가 제공자 약관 확인). 캐시가 흡수:
  반복되는 동일 주소는 TTL 동안 1회만 외부 호출. positive 결과 긴 TTL 로 재조회 억제. rate-limit
  초과 응답(429) → 에러 → field-omit(dormancy), batch 비차단.

---

## activation

이 method 가 구현·등록되면 **~13개 컴플라이언스 / sanctions / issuer-freeze 계열 catalog 정책**이
dormant → live 로 전환된다 (정확한 정책 id 목록·activation map 의 SSOT 는
`POLICY_RPC_METHODS.md` §4 — 본 method 가 거기 등록되면 그 표에 행이 추가된다).

- **primary deny 트리거**: `context.custom.sanctioned == true` 인 정책들 — 제재 대상 spender/recipient/
  counterparty 와의 approve / transfer / swap / borrow 차단.
- **fail-on-stale 메타 정책**: `context.custom.sanctionsFresh == false` 일 때 warn/fail — 명단이 오래되어
  신뢰할 수 없을 때 통과시키지 않도록. 이 정책은 `sanctionsFresh` leaf 투영이 같이 구현돼야 깨어난다.
- **issuer-freeze 게이트**: 제재 대상 관여 시 발행자-동결 시나리오를 트리거하는 정책의 입력 게이트로
  `sanctioned` 비트 재사용.

> ⚠️ `address.sanctions` 는 현재 method-catalog.json **부재**이고, 위 "~13" 은 컴플라이언스 덱 기준
> 추정 규모다 — catalog 에 해당 정책 fixture 가 실제로 추가/배선되기 전까지는 method 만으로 자동
> 활성되지 않는다. 정확한 정책 id 와 카운트는 catalog 에 정책이 들어온 뒤 §4 activation map 으로 확정.
> (덱 인용은 1차 출처가 아님 — 정량 카운트는 **출처 미확인**, catalog fixture 로 검증할 것.)

---

## primary-source references

- **OFAC Sanctions List Service** — SDN(Specially Designated Nationals) + Consolidated list 의 공식
  배포(SDN.XML / SDN.CSV, Digital Currency Address 필드 포함). 로컬 sanctions set 스냅샷의 1차 발행 소스.
  https://ofac.treasury.gov/sanctions-list-service
- **OFAC — Questions on Virtual Currency / Digital Currency Address FAQ** — 명단에 가상자산 주소가
  어떻게 등재·표기되는지(필드 의미)의 공식 설명. 갱신 주기·정확 스키마는 OFAC 발행물 기준으로 구현 시
  확정 (정량 SLA 는 **출처 미확인**). https://ofac.treasury.gov/faqs
- **EIP-2612** — `permit()` (서명 위임이 곧 allowance 가 되는 surface; spender 평가 동기).
  https://eips.ethereum.org/EIPS/eip-2612
- **EIP-712** — typed structured data signing (permit 서명 페이로드 기반).
  https://eips.ethereum.org/EIPS/eip-712
- **EIP-155** — chain id (param `chain_id` 정의). https://eips.ethereum.org/EIPS/eip-155
- **외부 screening 제공자 (Chainalysis / TRM Labs)** — sanctions screening API / on-chain oracle 후보.
  정확한 엔드포인트 · 인증 헤더 · 응답 스키마 · quota 는 **출처 미확인** (각 제공자 공식 docs 에서
  구현 시 확정).
- **In-repo 패턴 (1차, 코드)**:
  - `crates/policy-server/sync/src/sources/fetchers/registry.rs` — `RegistryFetcher` TTL 캐시(24h) +
    `build_url` 패턴 (로컬/GCS sanctions 스냅샷 재사용).
  - `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs` — `RestJsonOracleFetcher`
    env-auth(`std::env::var`) + `json_pointer` 추출 + `.timeout(...)` 패턴 (외부 screening API).
  - `crates/policy-server/sync/src/sources/fetchers/onchain.rs` — `OnchainViewFetcher` eth_call +
    selector 인코딩 (on-chain sanctions oracle 선택 경로).
  - `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1(fold), §2(record→scalar
    projection), §4(activation map), §5(dispatcher 부재 현황) — wire 계약 SSOT.
  - `schema/method-catalog.json` — 등록 스키마 형태 (현재 `address.sanctions` 부재 = aspirational 근거).

> **`address.reputation` 와의 관계**: 두 method 는 거의 동형 플럼빙(denylist/set + env-auth API +
> dormancy)이지만 **의미·출처·정책 분리가 다르다** — reputation 은 scam-휴리스틱(자산 보호, 사후·확률),
> sanctions 는 법적 명단 멤버십(컴플라이언스, 공식 발행·이산). 한 method 가 둘을 겸하지 않는다:
> reputation `label: "sanctioned"` 같은 보조 라벨이 있더라도 법적 sanctions 게이트는 OFAC-grounded
> 인 이 method 의 `sanctioned` 비트로만 결정해야 한다.
