# method: stat_window.swap_stats

status: existing (in method-catalog.json)

> 이 파일은 미래의 `/v1/rpc` 서버 구현자가 읽는 **구현 스펙**이다. wire interface 만이 아니라
> *어떻게* 만드는지를 기술한다. wire 계약/projection 제약/활성화 정책 목록의 1차 출처는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3b, §4) 이며,
> 재사용 가능한 plumbing 의 1차 출처는 `crates/policy-server/sync/src/sources/fetchers/{rpc,onchain,oracle}` 코드다.
> 모든 진술은 해당 코드/문서에 grounding 되어 있고, 검증 못 한 것은 "출처 미확인" 으로 명시했다.

## purpose

한 지갑(`owner`)의 **rolling-window 누적 swap 명목가치(USD)** 를 집계해 `context.custom.windowSwapUsd`
(Decimal) 로 주입한다. ScopeBall 의 velocity-cap 정책 — "오늘 누적 swap 이 $25k 넘으면 warn" — 은
*이번 한 건*의 크기가 아니라 **시간창 누적**에 한도를 건다 (Fireblocks TIMEFRAME-limit 류). 그런데
calldata 정적 디코드는 *현재 액션 하나*만 본다 — 과거 N시간의 swap 이력은 calldata 에 없다. 따라서 이
enrichment 가 지갑의 sliding window swap 이력을 합산해 누적 USD 를 돌려주고, 정책이 그 Decimal leaf 에
한도를 비교한다. ScopeBall 의 no-simulation 모델과 일관되게 이것은 **이력 fetch(+합산)** 이지 트랜잭션
시뮬레이션이 아니다.

`stat_window.snapshot` 과 같은 windowed-aggregation 엔진을 공유하되, **action 태그로 필터**해 swap leg 만
합산하는 변종이다 (snapshot = 전체 outflow → `windowOutflowUsd`; swap_stats = swap 만 → `windowSwapUsd`).

## interface

1차 출처: `schema/method-catalog.json` 의 `stat_window.swap_stats` 엔트리 + `POLICY_RPC_METHODS.md` §1·§2.

### params (각 `$.`-selector + type)

| param | type | required | defaultSelector | 설명 |
|---|---|---|---|---|
| `owner` | `String` | yes | `$.root.from` | window 를 집계할 지갑 주소 (서명자/송신자). |
| `action` | `String` | yes | `"swap"` | 집계 대상 action keyword (태그). swap-cap caller 는 `"swap"` 로 **고정 pin** 한다 — host 는 이 태그에 해당하는 leg 만 window 에 넣는다. |

> catalog manifest(`wallet/stat-window/daily-cumulative-swap-cap/manifest.json`)는 `owner = "$.root.from"`,
> `action = "swap"` 을 명시로 넘긴다. `defaultSelector` 의 `"swap"` 은 selector 가 아니라 **literal 상수**
> (앞에 `$.` 가 없음) — host 는 이를 그대로 태그 필터값으로 쓴다. window 길이(24h)는 catalog 설명
> ("sliding 24h window") 에 박혀 있고 param 화돼 있지 않다 — 서버 구현 상수다.

### result shape (record fields + types)

`method-catalog.json` 의 `returns` 는 `{ "kind": "record", "type": "WindowStats" }`. 이 메서드가
내보내야 하는 **필수 leaf** 는 정책이 읽는 `windowSwapUsd` 하나다. WindowStats 의 부가 필드(권장):

| field | type | 설명 |
|---|---|---|
| `windowSwapUsd` | Decimal (문자열) | **필수.** window 내 누적 swap 명목가치(USD). 정책이 읽는 유일한 leaf. |
| `count` | Long | optional. window 내 swap 건수. debug/reason 용 (catalog 설명: "volume + count"). |
| `windowSec` | Long | optional. 실제 적용한 window 길이(초). 86400 권장. |
| `windowStartTs` | Long | optional. window 시작 timestamp. staleness/debug 용. |

> `windowSwapUsd` 외 필드는 전부 부가정보다. v2 엔진은 record 를 통째로 받지 못하므로(아래 projection 참고)
> 이 부가 필드들은 host 가 채우든 말든 정책 verdict 에 영향이 없다 — 구현 1차 컷은 `windowSwapUsd` 만 채워도 된다.

### projection: `$.result.windowSwapUsd → Decimal` (record→scalar leaf — **mandatory**)

`POLICY_RPC_METHODS.md` §2 의 hard 제약: `materialize_v2` 는 **scalar** projection type 만
`context.custom.*` 에 받는다 — `String | Long | Bool | Decimal | Set<String>`. legacy record type
(`UsdValuation`, `WindowStats`) 은 제거됐다. 따라서 record(`WindowStats`)를 반환하더라도 manifest 가
`outputs[].from = "$.result.windowSwapUsd"`, `outputs[].type = "Decimal"` 로 **leaf scalar 까지 투영**
해야 한다. catalog manifest 가 정확히 이렇게 한다:

```json
"outputs": [{ "kind": "context", "field": "windowSwapUsd", "type": "Decimal",
              "from": "$.result.windowSwapUsd", "required": false }],
"custom_context": { "fields": { "windowSwapUsd": "decimal" } }
```

`outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 가 강제). `custom_context`
철자는 lowercase Cedar (`"decimal"`). 즉 host 가 어떤 record 모양으로 돌려주든, 정책이 실제로 보는 것은
`context.custom.windowSwapUsd : decimal` 하나다.

## data source(s)

이 메서드의 핵심 입력은 가격 oracle 이 아니라 **지갑의 과거 swap 이력** 이다. 본 repo 의 decode-time
fetcher 레이어는 *현재 액션 한 건*을 enrich 하도록 설계됐을 뿐, **시간창 이력 집계 인프라가 없다** —
따라서 이 메서드의 본체(history 스캔 + 분류 + USD 평가)는 대부분 NET-NEW 다. 단, USD 환산 단계는 기존
oracle fetcher 를 재사용한다.

### history backend (집계 원천) — **NET-NEW**

- 본 repo 의 `RpcRouter`(`fetchers/rpc/router.rs`)가 노출하는 메서드는 `eth_call` / `eth_balance` /
  `eth_block_number` / `eth_get_transaction_receipt` / `eth_gas_price` 뿐이다 — **`eth_getLogs` /
  account-history 스캔 / 인덱서 클라이언트가 없다**(코드 확인). 즉 "지난 24h 의 이 지갑 swap 들" 을 얻는
  경로가 repo 에 존재하지 않으므로 새로 만들어야 한다.
- 구현 후보(택1, 정확도/비용 trade-off):
  - **(a) 인덱서/분석 API** (예: Dune, Covalent, Etherscan account txlist, 또는 자체 인덱서). `owner`
    의 최근 트랜잭션을 받아 swap 만 분류 → 각 swap 의 inputToken×price 합산. off-chain HTTP, API-key
    필요 가능.
  - **(b) `eth_getLogs` 직접 스캔.** known DEX router/pool 의 Swap 이벤트를 `owner` 기준으로 window
    블록범위 스캔. RPC provider 의 `getLogs` range 한도/latency 에 종속. **RpcRouter 에 `eth_getLogs`
    추가가 선결.**
  - **(c) 로컬 누적 ledger.** SW/서버가 *자신이 통과시킨* swap 을 per-owner 로 기록(가장 가벼움). 단,
    ScopeBall 밖에서 서명된 swap(다른 지갑 UI)은 못 보는 **체계적 과소집계** — false-PASS 위험. 정직한
    한계로 문서화 필수.
- 어느 backend 든 산출물은 동일: `(owner, action="swap", window)` → 그 window 내 swap leg 의 목록
  `{token, rawAmount, ts}`.

### USD 환산 (per-leg) — EXISTING-FETCHER-REUSABLE

- window 내 각 swap leg 의 `token × rawAmount` 를 USD 로 환산하는 단계는 **`oracle.usd_value` 의 환산
  로직과 동일** — 같은 가격 fetcher 를 재사용한다:
  - `RestJsonOracleFetcher`(`fetchers/oracle/rest_json.rs`, CoinGecko REST) 또는
    `ChainlinkFetcher`(`fetchers/oracle/chainlink.rs`, on-chain `latestRoundData()`).
  - 토큰 `decimals` 가 leg 메타에 없으면 ERC-20 `decimals()` on-chain read —
    `OnchainViewFetcher`(`fetchers/onchain.rs`) 재사용.
- **NET-NEW 인 부분**: leg 별 `rawAmount × price ÷ 10^decimals` 를 합산하는 reduce. 단가 자체는 fetcher
  가 준다. (가격 평가 시점 = 각 leg 의 ts 기준이 정확하나, 단순화로 *현재가* 일괄 적용도 허용 — 아래
  heuristic limits 참조.)

### swap 분류 — **NET-NEW**

- backend 가 트랜잭션/로그를 주면 그중 무엇이 "swap" 인지 판정해야 한다. 가능하면 ScopeBall 의 v3
  declarative 디코더(`crates/policy-engine-wasm/src/declarative_exports.rs` → `Amm::Action::Swap`
  body) 를 재사용해 일관된 swap 정의를 쓰는 게 이상적이나, 이력 대량 재디코드는 비용이 크다. backend
  가 인덱서면 인덱서의 swap 라벨에 의존(정확도는 인덱서 종속). 이 분류 기준의 authority 는 `/v1/rpc`
  서버 구현자다.

## derivation algorithm

입력: `owner` (String, 지갑 주소), `action` (String, `"swap"` 으로 pin).

1. **window 경계 산정.** `now = clock` (서버 시각), `windowSec = 86400`(권장 상수), `windowStartTs =
   now - windowSec`. 블록범위 스캔이면 `windowStartTs` → 대략 시작블록 변환(`eth_block_number` +
   평균 블록타임 또는 이진탐색).
2. **이력 수집.** 선택한 history backend 로 `owner` 의 `[windowStartTs, now]` 트랜잭션/로그를 가져온다.
   backend 부재/에러 → **abort**(결과 없음 → dormancy contract, 아래).
3. **swap 필터.** `action == "swap"` 태그에 해당하는 leg 만 남긴다(swap 분류, 위). 빈 집합이면 누적 0 —
   단, **결과를 비울지 0 을 줄지 결정은 신중히** (아래 dormancy 주의: 임의 0 은 cap 우회를 만들 수 있으나,
   "이력을 정상 조회했고 실제 swap 이 없었다" 면 0 이 *정확한 사실*이다. abort 는 "조회 실패" 일 때만).
4. **per-leg USD 환산.** 각 swap leg 의 `inputToken × rawAmount` 를 oracle fetcher 로 USD 환산
   (`rawAmount / 10^decimals * price`). 부동소수 금지 — `Decimal` 십진 산술
   (`oracle.usd_value` / `chainlink.rs::scale_to_decimal` 와 동일 패턴).
5. **합산.** `windowSwapUsd = Σ leg_usd` (Decimal 합). `count = |legs|`.
6. **record 조립.** `{ windowSwapUsd, count, windowSec, windowStartTs }`. host fold 는 이 record 를
   `map[call_id]` 로 넣고, manifest projection `$.result.windowSwapUsd → Decimal` 이
   `windowSwapUsd` leaf 를 뽑는다.

### heuristic limits (정직한 한계)

- **이력 커버리지 = 메서드 커버리지.** backend 가 못 본 swap(다른 지갑 UI 로 서명, 인덱서 누락, getLogs
  range 한계) 은 누적에서 빠진다 → **과소집계 → false-PASS 가능**. 특히 로컬-ledger backend(c) 는
  ScopeBall 밖 swap 을 0으로 본다. 과장 금지: 이 메서드는 "ScopeBall 이 관측 가능한 범위의 swap 누적"
  이지 절대적 onchain 진실이 아니다.
- **가격 시점 근사.** leg 별 ts 시점 가격이 정확하나, 단순화로 *현재가* 를 일괄 적용하면 변동성 큰 토큰의
  과거 leg 가 부정확해진다. cap 은 "대략적 누적 한도" 이지 정밀 회계가 아니다.
- **window 경계 = 서버 시각.** sliding 24h 의 기준 `now` 는 서버 시각이다. 클라이언트/체인 시각과의
  skew, 블록→ts 변환 오차가 경계 근처 leg 의 포함/제외를 흔들 수 있다.
- **swap 정의 종속성.** "무엇이 swap 인가" 는 분류기(인덱서 라벨 또는 재디코드)에 종속 — multicall 내부
  swap, intent-fill, 비표준 router 는 분류기 커버리지에 따라 누락될 수 있다.
- **현재 액션 미포함 여부.** 이 window 가 *지금 서명하려는* swap 을 포함하는지(누적에 더해 비교할지)는
  정책 의도에 달렸다. catalog 정책은 `windowSwapUsd > 25000` 만 비교하므로, host 가 "과거 누적" 만 줄지
  "과거+현재" 를 줄지를 정해 reason 에 명시해야 한다(현재 건 포함이 cap 의도에 더 부합). 이 결정의
  authority 는 `/v1/rpc` 구현자다.

## on-chain calls

- **history backend 가 `eth_getLogs` 직접 스캔(b)이면**: window 블록범위에 대해 DEX Swap 이벤트
  `eth_getLogs` 다회(range chunk). **RpcRouter 에 `eth_getLogs` 미구현 — 선결 추가 필요.**
- **인덱서/분석 API(a) 이면**: on-chain 직접 호출 없음(off-chain HTTP). 블록↔ts 변환에 `eth_block_number`
  1회 정도만.
- **per-leg USD 환산 부산물**:
  - `source = "chainlink"` 단가면 leg 당 `latestRoundData()` `eth_call`(캐시로 토큰별 1회로 접힘).
  - 토큰 `decimals` 미상 시 ERC-20 `decimals()`(selector `0x313ce567`) read.
- **multicall?**: 동일 chain 의 여러 leg decimals/price read 는 `Multicall.aggregate3`
  (`fetchers/onchain.rs::fetch_batch`, `fetchers/rpc/multicall.rs`) 로 1회로 합치는 게 권장.

## caching / ttl

- **window aggregate cache key**: `(chain_id, owner, action="swap", windowBucket)`. value =
  `(windowSwapUsd: Decimal, count, windowStartTs)`. **owner-scoped** 이며 amount 와 무관.
- **ttl**: window 집계는 휘발성(새 swap 마다 변함) → 짧게, 권장 **15s~60s**. 너무 길면 방금 발생한 swap
  을 놓쳐 cap 우회(과소집계). **출처 미확인** (이 TTL 수치는 본 repo 코드에 명시 상수가 없는 권장값).
- **per-token 단가/decimals cache**: `oracle.usd_value` 와 공유 — 단가 `(chain, source, feed|coin-id)`
  TTL 30~60s, decimals `(chain, address)` 장수명.
- **위치**: `/v1/rpc` 서버 in-process 캐시. history backend 왕복(인덱서 HTTP 또는 다회 `getLogs`)이 이
  메서드의 가장 무거운 부분이고, per-action 은 orchestrator `HARD_TIMEOUT_MS = 8000` 예산 안에 들어야
  한다 — cache hit 이면 산술뿐, cold miss 의 다회-`getLogs`/대량 환산은 **예산 압박** 가능 → 인덱서 단일
  쿼리 backend 가 latency 측면에서 유리.

## failure & fallback (DORMANCY CONTRACT)

이 메서드의 catalog caller(`daily-cumulative-swap-cap`)는 `policy_rpc[].optional: true` +
`outputs[].required: false` (catalog enrichment 의 표준). 따라서:

- history backend 부재/에러, 인덱서 timeout, RPC/`getLogs` 실패, 단가 미확보, param selector 미해소 →
  **결과 record 에서 `windowSwapUsd` 필드를 내보내지 않는다** (또는 result 를 통째로 비운다).
- host fold 는 missing/`ok:false` 결과를 `map[call_id]` 에서 **drop** 한다
  (`POLICY_RPC_METHODS.md` §1 wire contract).
- 그 결과 `context.custom` 에 `windowSwapUsd` 필드가 **없음** → 정책의 `context.custom has windowSwapUsd`
  guard 가 **false** → 해당 swap-cap 정책은 **INERT** (verdict 미생성). dormant 정책은 false verdict 를
  만들지 않는다.
- **절대** verdict 를 뒤집을 수 있는 default 를 대입하지 않는다. 특히 **조회 실패 시 `windowSwapUsd = 0`
  금지** — `0` 은 cap 을 항상 통과시켜(`0 > 25000` = false) **false PASS** 를 만든다. 반대로 임의 큰 값은
  false WARN. 둘 다 금지. `0` 은 *오직* "이력을 정상 조회했고 실제 swap 이 0건" 일 때만 정당한 사실값이다
  (조회 실패와 구분).
- `optional: true` 이므로 missing input 은 batch hard-fail 이 아니라 **pass 로 degrade**(해당 정책만
  inert)한다.
- 요약: **조회 실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. fail-closed
  방향(deny)으로 흐르지 않는다. (이는 enrichment 의 일반 계약이고, deny-closed 인 HyperLiquid 경로와 무관
  — 이 메서드는 velocity-cap enrichment 일 뿐 venue 차단 로직이 아니다.)

## auth / cost / rate-limit

- **history backend (a) 인덱서/분석 API**: API-key 가능(`RestAuthConfig{header_name, env_var}` env-var
  resolve 패턴 재사용 가능, `rest_json.rs` 선례). per-call cost = `owner` window 쿼리 1회(캐시 miss 시).
  rate-limit 은 provider 플랜 종속(**출처 미확인**). 캐시(TTL 15~60s)가 동일 owner 반복을 1회로 접어
  압력을 줄인다.
- **history backend (b) `eth_getLogs`**: API-key 불필요(public RPC 가능)하나 provider 의 `getLogs`
  block-range 한도/rate-limit 적용. per-call cost = window range chunk 수 × `getLogs`. 무거움 →
  인덱서 backend 대비 latency 불리.
- **USD 환산**: `oracle.usd_value` 와 동일 — CoinGecko free-tier 분당 한도(**출처 미확인**, 플랜 종속) /
  Chainlink public RPC rate-limit. 단가·decimals 캐시가 토큰별 1회로 접는다.

## activation

이 메서드를 구현하면 다음 **1개** catalog 정책이 dormant 에서 해제된다
(`POLICY_RPC_METHODS.md` §4 activation map — 단, swap_stats 는 §3b "available but not used by the
catalog" 로도 분류돼 있음; 실제 catalog 에는 이 정책이 swap_stats 를 참조한다):

- `daily-cumulative-swap-cap` (wallet/stat-window) — swap 한 건이 지갑의 24h 누적 swap 을 $25k 초과로
  밀어올리면 **warn**.

> **참고:** `stat_window.swap_stats` 는 `method-catalog.json` 에 이미 등록(`origin: "bundled"`)돼 있어
> Cedar 컴파일/정책 install 은 오늘도 된다. dormant 인 이유는 `/v1/rpc` 서버 본체(dispatcher + history
> backend)가 미구현이기 때문이다(`POLICY_RPC_METHODS.md` §5: "no `/v1/rpc` method dispatcher in-repo").
> 자매 메서드 `stat_window.snapshot` 은 `daily-cumulative-approval-cap`(→`windowOutflowUsd`)이 쓰며,
> 두 메서드는 동일한 windowed-aggregation 엔진을 공유한다(snapshot = 전체 outflow, swap_stats = swap 필터).

## primary-source references

- ScopeBall enrichment wire 계약 / projection 제약 / activation map:
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3b, §4, §5) — repo 내부 1차.
- 메서드 카탈로그 엔트리(params/returns/origin): `schema/method-catalog.json` `stat_window.swap_stats`
  (및 자매 `stat_window.snapshot`).
- catalog 정책/manifest (activation·projection 실제 shape):
  - `crates/policy-engine/tests/fixtures/policy_catalog_v2/wallet/stat-window/daily-cumulative-swap-cap/manifest.json`
  - `.../daily-cumulative-swap-cap/policy.cedar`
- 재사용/선결 plumbing (1차 = 코드):
  - `crates/policy-server/sync/src/sources/fetchers/rpc/router.rs` (`RpcRouter` — 현 노출 메서드 집합;
    `eth_getLogs` 부재 확인 → backend (b) 선결 추가 지점).
  - `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs` (`RestJsonOracleFetcher` — per-leg USD 단가, CoinGecko).
  - `crates/policy-server/sync/src/sources/fetchers/oracle/chainlink.rs` (`ChainlinkFetcher` / `scale_to_decimal` — on-chain 단가 + 십진 스케일링 패턴).
  - `crates/policy-server/sync/src/sources/fetchers/onchain.rs` (`OnchainViewFetcher` / `fetch_batch` — ERC-20 decimals fallback, Multicall 배치).
  - `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs` (`Multicall::aggregate3`).
- 동기 motivation (use case): Fireblocks velocity / TIMEFRAME transaction-amount limit 정책 (지갑 velocity
  control). Fireblocks 공식 정책 문서 정확 URL/세부 한도는 **출처 미확인** (본 repo 외부, 미검증).
- EIP-155 (chain id) — https://eips.ethereum.org/EIPS/eip-155; ERC-20 `decimals()` — https://eips.ethereum.org/EIPS/eip-20.
