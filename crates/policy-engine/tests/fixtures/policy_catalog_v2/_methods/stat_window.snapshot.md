# method: stat_window.snapshot

status: existing (in method-catalog.json — `stat_window.snapshot`, `returns.kind = "record"`,
`type = "WindowStats"`, `params.owner` `defaultSelector = "$.root.from"`, `origin = "bundled"`).
단, **데이터 백엔드는 미배선** — 메서드 시그니처는 등록돼 있으나 실제 집계 소스가 없어 사실상 dormant
(아래 "honest limit / dormancy" 참조).

> 이 파일은 미래의 `/v1/rpc` 서버 구현자가 읽는 **구현 스펙**이다. wire interface 만이 아니라
> *어떻게* 만드는지를 기술한다. wire 계약/projection 제약/활성화 정책 목록의 1차 출처는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3b, §6) 이며,
> 메서드 시그니처의 1차 출처는 `schema/method-catalog.json` (`stat_window.snapshot`) 다.
> 재사용 가능한 plumbing 의 1차 출처는 `crates/policy-server/sync/src/sources/fetchers/{onchain,rpc}` 코드다.
> 모든 진술은 해당 코드/문서에 grounding 되어 있고, 검증 못 한 것은 "출처 미확인" 으로 명시했다.

## purpose

한 actor(지갑) 의 **시간 윈도우 누적 OUTFLOW** 를 집계한다. 정적 per-action 엔진은 *지금 이 한 건* 의
calldata/signature 만 본다 — 직전 시각에 같은 지갑이 얼마를 내보냈는지(cumulative-over-time) 는 구조적으로
보이지 않는다. ScopeBall 의 velocity / structuring 류 정책 — "단건은 작아도 하루 누적 OUTFLOW 가 $10k 를
넘는 transfer/approve 는 warn" — 은 바로 이 **윈도우 누적값** 을 필요로 한다. per-tx cap 은 천 번에 걸친
조금씩-빼가기(drain-by-a-thousand-cuts) 를 놓치지만, rolling-window USD ceiling 은 aggregate 를 잡는다
(`daily-cumulative-transfer-cap` policy.cedar 의 의도 주석 인용).

이 enrichment 가 actor 의 최근 OUTFLOW 합을 가져와 `context.custom.windowOutflowUsd` (Decimal) 로 주입하고,
정책이 그 Decimal leaf 에 한도(`> 10000.0000`)를 비교한다. ScopeBall 의 no-simulation 모델과 일관되게
이것은 **이미 인덱싱된 history 에 대한 집계(fetch+sum)** 이지 트랜잭션 시뮬레이션이 아니다 — 단, 다른
catalog enrichment(`oracle.usd_value`, `approval.allowance` 등)가 "한 번의 fetch" 인 것과 달리, 이 메서드는
**서버측 history 인덱싱 인프라를 전제** 한다 (그 부재가 이 메서드의 진짜 dormancy 원인이다, 아래).

## interface

1차 출처: `schema/method-catalog.json` 의 `stat_window.snapshot` 엔트리 + `POLICY_RPC_METHODS.md` §1·§2·§3b.

### params (각 `$.`-selector + type)

| param | type | required | defaultSelector | 설명 |
|---|---|---|---|---|
| `owner` | `String` | yes | `$.root.from` | OUTFLOW 를 집계할 actor 주소. catalog manifest 둘 다 `"owner": "$.root.from"` 로만 호출 — 즉 서명/송신 주체 1개가 유일 입력이다. |

> **window 파라미터 부재 (정직한 갭).** `method-catalog.json` 의 `stat_window.snapshot.params` 에는 `owner`
> **하나뿐** — 윈도우 길이(예: 24h)나 통화/체인 스코프를 넘기는 param 이 없다. catalog 정책의 의도(주석·
> `@reason`)는 "daily / $10k/day" 지만, 그 **24h·USD·all-chain 스코프는 서버 구현이 박는 상수** 이지 호출자가
> param 으로 넘기지 않는다. 구현자는 이 윈도우/통화 정의를 서버측에 고정하고 문서화해야 한다. (per-window
> 파라미터화가 필요해지면 catalog 엔트리에 `window`/`unit` param 을 추가하는 것은 별건 변경이다.)

### result shape (record fields + types)

`method-catalog.json` 의 `returns` 는 `{ "kind": "record", "type": "WindowStats" }`. 정책이 투영하는 leaf
기준 최소 필드:

| field | type | 설명 |
|---|---|---|
| `windowOutflowUsd` | Decimal (문자열) | **필수.** 윈도우(예: 24h) 동안 actor 의 누적 OUTFLOW USD 합. catalog 정책이 읽는 유일한 leaf. |
| `windowTxCount` | Long | optional. 같은 윈도우의 OUTFLOW 트랜잭션 건수. **현재 catalog 정책은 미사용** — 미래 "윈도우당 N건 초과 warn" 류 정책을 위한 부가 leaf. |
| `windowStartTs` | Long | optional. 집계 윈도우 시작 시각(epoch s). debug/staleness 표면화용. |
| `source` | String | optional. 집계에 쓴 history 소스 식별자(indexer 이름 등). debug 용. |

> `windowOutflowUsd` 외 필드는 전부 부가정보다. v2 엔진은 record 를 통째로 받지 못하므로(아래 projection
> 참고) 이 부가 필드들은 host 가 채우든 말든 정책 verdict 에 영향이 없다 — 구현 1차 컷은 `windowOutflowUsd`
> 만 채워도 두 활성 정책이 동작한다.

### projection: `$.result.windowOutflowUsd → Decimal` (record→scalar leaf — **mandatory**)

`POLICY_RPC_METHODS.md` §2 의 hard 제약: `materialize_v2` 는 **scalar** projection type 만
`context.custom.*` 에 받는다 — `String | Long | Bool | Decimal | Set<String>`. legacy record type
(`UsdValuation`, `WindowStats`) 은 v2 context 에서 제거됐다. 따라서 record 를 반환하더라도 manifest 가
leaf scalar 까지 투영해야 한다. 두 활성 catalog manifest 가 실제로 박은 것:

```json
"outputs": [{ "kind": "context", "field": "windowOutflowUsd", "type": "Decimal",
              "from": "$.result.windowOutflowUsd", "required": false }],
"custom_context": { "fields": { "windowOutflowUsd": "decimal" } }
```

`outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 가 강제). `outputs[].type` 은
capitalized (`"Decimal"`), `custom_context` 철자는 lowercase Cedar (`"decimal"`). 즉 host 가 어떤 record
모양으로 돌려주든, 정책이 실제로 보는 것은 `context.custom.windowOutflowUsd : decimal` 하나다. (선택적
`windowTxCount` 를 쓰는 미래 정책이라면 `$.result.windowTxCount → Long` leaf 를 추가로 투영하면 된다 —
별도 leaf, 별도 정책.)

## data source(s)

이 메서드는 **단일 fetcher 가 아니라 actor history 에 대한 윈도우 집계(sum)** 다. 그래서 다른 catalog
메서드와 다르게 **재사용 가능한 fetcher 가 사실상 없고**, 본체가 NET-NEW 다.

### NET-NEW (본체) — actor OUTFLOW history 인덱싱 + 윈도우 합산

- **무엇이 필요한가**: `owner` 의 최근 윈도우(예: 24h) OUTFLOW 이벤트들을 모은 뒤 USD 로 환산해 합산.
  OUTFLOW = 그 지갑에서 **나간** 가치 (ERC-20 `Transfer(from=owner)` 로그 + native value 송금 + approve
  로 노출된 spend 등 — 정확한 OUTFLOW 정의는 정책 의도에 맞춰 서버가 고정해야 함).
- **왜 NET-NEW 인가 (코드로 확인)**: 오늘 repo 의 `RpcRouter` 는 `eth_call` 만 노출하고
  (`crates/policy-server/sync/src/sources/fetchers/rpc/router.rs` — `getLogs`/`getTransactionCount` 없음),
  `crates/policy-server/sync/.../fetchers/` 어디에도 **per-actor 시계열 outflow store** 가 없다
  (`grep outflow|window_stats|tx_history|rolling_window` → policy-server 에서 매칭 0, hyperliquid 무관 항목만).
  즉 단가(`oracle.usd_value`) 처럼 한 fetcher 를 호출해 끝나는 게 아니라, **history 인덱스 + 윈도우 합산
  레이어** 자체를 구축해야 한다.
- **어디에 둘 것인가**: policy-server 의 wallet store / state 레이어 + sync Orchestrator
  (`crates/policy-server/sync/src/runtime/orchestrator.rs`) 류의 백그라운드 인덱서. Orchestrator 는
  현재 fetch **스케줄러** 이지 history store 가 아니므로 (코드 확인), 인덱싱 대상(actor→윈도우 outflow
  롤업) 을 새로 정의해야 한다. **출처 미확인** — 이 store 의 구체 스키마/배치 전략은 본 repo 에 아직 없다.

### EXISTING-FETCHER-REUSABLE (보조 plumbing 만)

집계 *본체* 는 NET-NEW 지만, 그 안의 USD 환산·on-chain 보조에는 기존 부품을 재사용한다:

- **USD 환산**: OUTFLOW 이벤트의 토큰 raw amount → USD 는 `oracle.usd_value` 메서드와 **동일 산술/단가
  소스** (`oracle/rest_json.rs` `RestJsonOracleFetcher` / `oracle/chainlink.rs` `ChainlinkFetcher`) 를
  재사용한다. 윈도우 내 각 outflow 를 단가×수량÷10^decimals 로 환산해 합산. (즉 `stat_window.snapshot` ≈
  "윈도우 내 모든 outflow 에 `oracle.usd_value` 를 적용한 합".)
- **on-chain 보조 read**(decimals 확보 등): `OnchainViewFetcher` / `Multicall.aggregate3`
  (`fetchers/onchain.rs::fetch_batch`, `fetchers/rpc/multicall.rs`) 재사용 — `decimals()` (`0x313ce567`),
  `balanceOf` 셀렉터 인코딩 precedent 이 `onchain.rs` 테스트에 있다.
- **재사용 안 되는 것**: history **수집** 자체 (로그 인덱싱/스캔). 위 fetcher 들은 전부 *현재 상태* point-read
  (`eth_call`) 또는 단가 fetch 이지, **시계열 이벤트 수집기가 아니다**. 이 부분이 진짜 NET-NEW 다.

## derivation algorithm

입력: `owner` (String). 윈도우 길이·통화·체인 스코프는 서버 상수(권장 24h / USD / 대상 체인 집합).

1. **윈도우 경계 확정.** `windowStartTs = now - WINDOW_SECS` (권장 `WINDOW_SECS = 86400`). `now` 는 서버
   시각 (catalog 의 `clock.now` 와 동일 daemon clock 개념). 윈도우 정의는 서버에 고정·문서화.
2. **OUTFLOW 이벤트 수집.** indexed history store 에서 `owner` 가 보낸 outflow 이벤트들을
   `[windowStartTs, now]` 로 필터 (ERC-20 `Transfer(from=owner)` + native value send 등 — 정의 고정).
   store 가 비었거나(인덱싱 미완) 조회 실패 → **abort** (결과 없음 → dormancy contract, 아래).
3. **USD 환산 + 합산.** 각 이벤트의 `(asset, raw amount)` → `oracle.usd_value` 와 동일 경로로 단가·decimals
   확보 후 `usd_i = (amount_i / 10^d_i) * price_i`. `windowOutflowUsd = Σ usd_i` (Decimal 십진 산술,
   **부동소수 금지** — `chainlink.rs::scale_to_decimal` 의 십진-문자열 스케일링 패턴 준수). 동시에
   `windowTxCount = count(events)`.
4. **record 조립.** `{ windowOutflowUsd, windowTxCount, windowStartTs, source }`. host fold 는 이 record
   를 `map[call_id]` 로 넣고, manifest projection `$.result.windowOutflowUsd → Decimal` 이
   `windowOutflowUsd` leaf 를 뽑는다.

### heuristic limits (정직한 한계)

- **history 인덱싱 의존 = 이 메서드의 진짜 dormancy 원인.** 단가/allowance 류와 달리 한 번의 fetch 로
  끝나지 않는다. per-actor outflow 인덱스가 없으면 step 2 가 항상 abort → 정책 영구 INERT. 시그니처는
  catalog 에 등록돼 있어도(`origin: "bundled"`), **데이터 백엔드가 배선되기 전까지 실질 dormant** 다.
  과장 금지: "registered" ≠ "functional".
- **윈도우 커버리지 = OUTFLOW 정의 커버리지.** OUTFLOW 를 ERC-20 `Transfer(from)` 로만 잡으면 internal
  transfer / native value send / DEX 경유 우회 outflow 를 놓칠 수 있다. 합이 *과소* 추정되면 cap 우회
  (false pass) 위험 — 정의 범위를 명시·문서화해야 한다.
- **단가 staleness 전파.** 합산 단가는 `oracle.usd_value` 와 같은 spot 단가 1점에 의존 — depeg/flash-loan
  왜곡 보정 없음. 윈도우 합도 그 부정확성을 상속한다.
- **체인 스코프.** "daily" 가 멀티체인 누적인지 단일체인인지는 서버 상수로 박힌다. cross-chain drain 을
  잡으려면 대상 체인 집합을 명시해야 한다 (param 부재 → 호출자가 고를 수 없음, 위 interface note).
- **다중 catalog caller 의 같은 leaf.** `transfer`/`approve` 두 정책이 같은 `windowOutflowUsd` leaf 를
  공유한다 — 같은 actor 의 한 action 에서 둘 다 활성이어도 윈도우 집계는 1회로 캐시 흡수(아래).

## on-chain calls

- **직접 on-chain (이 메서드 본체)**: 권장 구현은 **이미 인덱싱된 history store 조회** 라 per-action
  on-chain 호출 0회 가 이상적이다 (point-read 가 아니라 시계열 합산이므로 매 action 마다 로그를 새로
  스캔하는 것은 `HARD_TIMEOUT_MS = 8000` 예산상 비현실적). history 는 백그라운드 인덱서가 미리 채운다.
- **보조 read (USD 환산 시)**: `oracle.usd_value` 와 동일 — chainlink source 면 `latestRoundData()`
  (`AggregatorV3Interface`), decimals 부재 시 ERC-20 `decimals()` (`0x313ce567`). 이들은 윈도우 *집계
  시점*(백그라운드)에 일어나면 되고, per-action 경로에서는 캐시된 합만 읽는다.
- **multicall?**: 인덱싱 단계에서 여러 토큰 단가/decimals 를 한 chain·한 batch 로 묶을 때만
  `Multicall.aggregate3` (`fetchers/onchain.rs::fetch_batch`) 권장. per-action read 경로엔 불필요.

## caching / ttl

- **집계 cache key**: `(owner, window_secs)` → `{ windowOutflowUsd: Decimal, windowTxCount: Long,
  windowStartTs }`. catalog 가 `owner` 만 넘기므로 window 는 서버 상수, key 는 사실상 `owner` 단위.
- **ttl**: 윈도우 합은 새 outflow 가 생기거나 윈도우가 굴러갈 때만 바뀐다. 짧은 TTL(권장 30s~60s) 로
  per-action 반복을 흡수하되, 같은 지갑이 *방금* 큰 outflow 를 했으면 캐시가 그것을 너무 늦게 반영하지
  않도록 인덱서 갱신 이벤트로 무효화하는 게 이상적. **출처 미확인** (이 TTL 수치는 본 repo 코드에 명시
  상수 없는 권장값).
- **위치**: `/v1/rpc` 서버 in-process 캐시 + 백그라운드 history 인덱스. per-action 경로는 캐시 hit 이면
  산술/조회뿐 — `HARD_TIMEOUT_MS = 8000` (orchestrator) 예산 내. 같은 action 에서 transfer·approve cap 이
  동시에 활성이어도 `(owner)` 캐시가 집계 1회로 접는다 (두 manifest 가 동일 params 로 같은 `owner` 를 호출).

## failure & fallback (DORMANCY CONTRACT)

두 활성 catalog caller 는 `policy_rpc[].optional: true` + `outputs[].required: false` (catalog enrichment
표준; 두 manifest 모두 그렇게 박힘). 따라서:

- history 미인덱싱(백엔드 부재), store 조회 실패, 단가 환산 실패, timeout, `owner` selector 미해소 →
  **결과 record 에서 `windowOutflowUsd` 필드를 내보내지 않는다** (또는 result 를 통째로 비운다).
- host fold 는 missing/`ok:false` 결과를 `map[call_id]` 에서 **drop** 한다
  (`POLICY_RPC_METHODS.md` §1 wire contract).
- 그 결과 `context.custom` 에 `windowOutflowUsd` 필드가 **없음** → 정책의
  `context.custom has windowOutflowUsd` guard 가 **false** → 해당 velocity-cap 정책은 **INERT**
  (verdict 미생성). dormant 정책은 false verdict 를 만들지 않는다 (policy.cedar guard 가 정확히 이 has-check
  를 함).
- **절대** verdict 를 뒤집을 수 있는 default(예: `windowOutflowUsd = 0` 이나 `= ∞`) 를 대입하지 않는다.
  `0` 이면 cap 우회(false pass), 큰 값이면 false warn — 둘 다 금지. `optional: true` 이므로 missing input 은
  batch hard-fail 이 아니라 **pass 로 degrade**(해당 정책만 inert)한다.
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. 이 메서드는 wallet velocity-cap
  enrichment 일 뿐이고, deny-closed 인 HyperLiquid venue 경로와 무관하다. ⚠️ 단, 이 메서드의 "실패" 는
  대부분 일시적 fetch 오류가 아니라 **구조적(백엔드 미배선)** 이라는 점에 유의 — 즉 history 인덱서가
  배선되기 전까지 두 정책은 *항상* inert 다.

## auth / cost / rate-limit

- **history 인덱서**: 외부 indexer/노드 API(`eth_getLogs` 스캔 또는 third-party indexer) 에 의존하면 그
  provider 의 auth/rate-limit 이 적용된다. per-actor 백그라운드 인덱싱이라 **per-action 경로에는 비용 0
  에 가깝다** (캐시된 합 읽기). 구체 provider/키 정책은 **출처 미확인** (백엔드 미선정).
- **단가 환산 비용**: `oracle.usd_value` 와 동일 — CoinGecko HTTP(무인증 free tier fallback 가능) 또는
  Chainlink RPC. 윈도우 집계 시점에 토큰별 단가 캐시(TTL 30~60s)가 반복 호출을 1회로 접는다.
- **rate-limit 압력**: per-actor 집계 캐시(`owner` 단위)가 동일 지갑의 연속 action 을 1회 집계로 흡수해
  indexer/oracle rate-limit 압력을 크게 줄인다.

## activation

이 메서드의 **데이터 백엔드(history 인덱서 + 윈도우 합산)를 배선** 하면 다음 2개 catalog 정책이 dormant
에서 해제된다 (`POLICY_RPC_METHODS.md` §3b·§6, 그리고 두 manifest 의 `method: "stat_window.snapshot"`):

- `daily-cumulative-transfer-cap` (wallet/stat-window) — trigger `action.tag == "erc20_transfer"`,
  guard `windowOutflowUsd > decimal("10000.0000")` → `@severity("warn")`.
- `daily-cumulative-approval-cap` (wallet/stat-window) — trigger `action.tag == "erc20_approve"`,
  동일 leaf·동일 $10k/day guard → warn.

> **note (`POLICY_RPC_METHODS.md` §3b 와의 관계):** 그 문서는 `stat_window.snapshot` 을 "available 이나
> catalog 미사용(record)" 으로 적었다 — 작성 시점 기준. 그러나 현 catalog 의 두 stat-window manifest 가
> 실제로 `method: "stat_window.snapshot"` 을 참조하므로, 이 메서드는 **이제 2개 정책의 활성 의존성** 이다
> (코드/manifest 가 1차 — `daily-cumulative-{transfer,approval}-cap/manifest.json`). 세 번째 형제
> `daily-cumulative-swap-cap` 은 `stat_window.swap_stats`(별 메서드, 별 spec) 를 쓴다.
> `windowTxCount` leaf 는 현재 어느 정책도 안 쓴다(미래용 부가 leaf).

## primary-source references

- ScopeBall enrichment wire 계약 / projection 제약 / 활성화 맵:
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3b, §6) — repo 내부 1차.
- 메서드 카탈로그 엔트리(params/returns/origin): `schema/method-catalog.json` `stat_window.snapshot`
  (`returns.kind = "record"`, `type = "WindowStats"`, `params.owner.defaultSelector = "$.root.from"`).
- 활성 정책(trigger/projection/guard 의 1차):
  - `crates/policy-engine/tests/fixtures/policy_catalog_v2/wallet/stat-window/daily-cumulative-transfer-cap/{manifest.json,policy.cedar}`.
  - `crates/policy-engine/tests/fixtures/policy_catalog_v2/wallet/stat-window/daily-cumulative-approval-cap/{manifest.json,policy.cedar}`.
- 재사용 plumbing (1차 = 코드; 집계 *본체* 는 NET-NEW, 보조만 재사용):
  - `crates/policy-server/sync/src/sources/fetchers/onchain.rs` (`OnchainViewFetcher` / `fetch_batch` — decimals 등 보조 read).
  - `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs` (`Multicall::aggregate3`).
  - `crates/policy-server/sync/src/sources/fetchers/rpc/router.rs` (`RpcRouter::eth_call` — `getLogs`/`getTransactionCount` 부재 = history 수집이 NET-NEW 임을 입증).
  - `crates/policy-server/sync/src/sources/fetchers/oracle/{rest_json.rs,chainlink.rs}` (윈도우 USD 환산은 `oracle.usd_value` 와 동일 단가 소스 재사용).
  - `crates/policy-server/sync/src/runtime/orchestrator.rs` (sync Orchestrator — 현재 fetch 스케줄러; history store 아님 → 인덱싱 레이어 신규 정의 필요. 스키마 **출처 미확인**).
- velocity/timeframe 정책 개념의 외부 1차: Fireblocks "TIMEFRAME"/velocity policy
  (정확한 윈도우 정의·임계값은 본 repo 와 무관하며 **출처 미확인** — catalog 의 $10k/day 는 우리 정책의 자체 상수).
