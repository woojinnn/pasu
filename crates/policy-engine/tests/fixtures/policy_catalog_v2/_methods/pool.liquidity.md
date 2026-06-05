# method: pool.liquidity

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> Referenced by the catalog manifest
> `action/swap/lp-low-liquidity/manifest.json` (`policy_rpc[0].method = "pool.liquidity"`),
> but **not** present in `schema/method-catalog.json`. The method is **dormant**: until the
> `/v1/rpc` dispatcher serves it, the projected field `poolVol24hUsd` is simply absent from
> `context.custom`, so the `lp-low-liquidity` guard is `false` and the policy is inert. Registering
> the method in `method-catalog.json` (alongside writing the handler) is part of implementing it.

---

## purpose

`pool.liquidity` 는 사용자가 유동성을 공급하려는(`add_liquidity`) **AMM 풀의 시장 깊이(market
depth) 지표** — 24h 거래량(USD), TVL(USD), 풀 생성 후 경과일 — 를 fetch 해서 정책 context 에
주입한다. 정적 calldata 만으로는 "이 풀이 얼마나 거래되는가 / 빠져나올 수 있는가" 를 알 수 없다
(풀 주소·deposit 금액은 디코드되지만 시장 활동성은 off-chain 시계열 데이터다). thinly-traded 풀에
LP 를 넣으면 (1) 청산/exit 시 슬리피지가 크고 (2) impermanent loss 노출이 높다. 단일 카탈로그
정책 `lp-low-liquidity` (I4, severity `warn`) 가 `poolVol24hUsd < $10,000` 일 때 사용자에게 경고
모달을 띄우는 데 이 fact 하나만 필요로 한다. record 전체가 아니라 **vol24hUsd 한 스칼라** 가
verdict 를 결정한다 (나머지 두 필드는 미래 정책/표시용 부가 정보).

## interface

**Params** (manifest `policy_rpc[].params`, dot-notation selectors — no array indexing/wildcards):

| param | selector | type | meaning |
|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long (eip155 numeric) | 풀이 사는 체인. `eip155:<id>` 의 숫자부 |
| `venue` | `$.action.venue` | record (`AmmVenue`) | 풀 식별자 — tagged enum: `{ "<VariantTag>": { chain, pool \| pool_id, ... } }` |

> `$.action.venue` 는 `AddLiquidityAction.venue: AmmVenue`
> (`crates/policy-server/asset-model/action/src/amm/mod.rs:90`) 의 ActionView 투영이다. tagged
> 객체이며 variant 별로 풀 주소를 다르게 보관한다:
> - `UniswapV2 / UniswapV3 / SushiV2 / CurveV1 / CurveV2 / TraderJoeLB` → `.pool` (Address)
> - `UniswapV4 / BalancerV2 / BalancerV3` → `.pool_id` (bytes32 hex) — pool **address** 가 아니라
>   풀 id 다. 핸들러는 variant tag 를 보고 lookup key 를 분기해야 한다 (UniV4/Balancer 는 pool_id,
>   나머지는 pool address). 모든 variant 가 `.chain` 도 들고 있으나 `chain_id` param 으로 중복
>   전달되니 핸들러는 둘이 일치하는지 sanity-check 만 하면 된다.
> - **honest limit**: `venue` 가 핸들러가 모르는 variant (예: 신규 DEX) 거나 indexer 가 그 풀을
>   모르면 → fact 없음 → 필드 미방출 (아래 dormancy contract).

**Result shape** (host 가 `$.result` 로 unwrap 해 반환하는 record):

| field | type | meaning |
|---|---|---|
| `tvlUsd` | Decimal (string) | 풀 양쪽 reserve 의 현재 USD 합 (total value locked) |
| `vol24hUsd` | Decimal (string) | 최근 24h rolling swap 거래량, USD |
| `ageDays` | Long | 풀 생성(첫 mint/배포) 후 경과일 |

**Projection** (record → scalar leaf — v2 에는 record ProjectionType 이 없으므로 **필수**):

- `$.result.vol24hUsd -> Decimal`  ⟶ `context.custom.poolVol24hUsd` *(카탈로그가 실제로 쓰는 단 하나)*
- `$.result.tvlUsd    -> Decimal`  *(미래 TVL-기반 정책용; 현 카탈로그 미사용)*
- `$.result.ageDays   -> Long`     *(미래 "갓 생성된 풀" 정책용; 현 카탈로그 미사용)*

> 현 `lp-low-liquidity` manifest 는 `outputs` 에 `vol24hUsd → poolVol24hUsd(Decimal)` **하나만**
> 선언한다. `outputs[].field` ⇄ `custom_context.fields` 는 1:1 (ManifestV2::validate 강제).
> Decimal 값은 string 으로 직렬화 (rust_decimal `.normalize().to_string()` 패턴, `oracle.rest_json`
> 과 동일) — JSON number 의 f64 정밀도 손실을 피한다.

## data source(s)

**NET-NEW plumbing** — 재사용 가능한 정확히-맞는 fetcher 는 없다. 가장 가까운 기존 코드:

- **EXISTING (패턴 재사용 가능, 직접 호출 불가)**:
  `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs`
  (`RestJsonOracleFetcher`) — base_url + path + JSON-pointer 추출 + env-기반 auth header +
  conservative HTTP timeout + reqwest client 패턴. `pool.liquidity` 의 subgraph/indexer HTTP
  핸들러는 이 구조(URL 조립 → GET/POST → `serde_json::Value::pointer` 추출 → Decimal 파싱)를
  그대로 본떠 작성하면 된다. 단 이 fetcher 는 `PriceFetcher` trait (단일 `Decimal` 반환) 라
  3-필드 record 를 못 돌려주므로 **그대로는 못 쓴다** — 별도 핸들러가 필요하다.
- **EXISTING (venue HTTP 패턴)**:
  `crates/policy-server/sync/src/sources/fetchers/venue/uniswap.rs` (`UniswapFetcher`) —
  venue API 로 POST 후 `first_path`/`value_at` 로 여러 후보 JSON 경로를 순차 탐색하는 패턴이
  subgraph 응답 파싱에 유용하다 (`vol24hUsd` 가 응답마다 `volumeUSD` / `volume24hUSD` 등으로
  키 이름이 다를 수 있음).
- **EXISTING (on-chain fallback 플럼빙)**:
  `crates/policy-server/sync/src/sources/fetchers/onchain.rs` (`OnchainViewFetcher` +
  `rpc/multicall.rs` `aggregate3`) — TVL 을 reserve 기반으로 직접 계산하는 fallback (아래
  derivation step 4) 에 재사용 가능. `OnchainCall::from_source` 가 selector+args 인코딩,
  `fetch_batch` 가 multicall3 일괄 호출을 제공한다.

**1차 데이터 소스 (구현자 선택)**:

1. **DEX 분석 subgraph / indexer** (권장, off-chain, primary 경로): 각 DEX 공식 subgraph 가
   풀별 시계열 (volumeUSD, totalValueLockedUSD, createdAtTimestamp) 를 노출한다 — Uniswap V2/V3
   official subgraph, Curve, Balancer 등. `venue` variant tag → 해당 subgraph endpoint 매핑
   테이블이 필요하다. **NET-NEW**: venue→subgraph 라우팅 테이블 + GraphQL query 본문 + record
   파서.
2. **on-chain reserve 읽기 (TVL fallback)**: subgraph 부재/실패 시 `vol24hUsd` 는 산출 불가지만
   `tvlUsd` 는 reserve × 가격으로 근사할 수 있다 (아래 derivation). **vol24hUsd 가 없으면 카탈로그
   정책은 어차피 dormant** 이므로 on-chain fallback 은 TVL-기반 미래 정책에만 의미가 있다.

## derivation algorithm

목표: `(chain_id, venue)` → `{ tvlUsd, vol24hUsd, ageDays }`.

1. **venue 정규화**: `venue` tagged 객체에서 `(variant_tag, pool_key)` 추출.
   `pool_key = pool` (address-기반 variant) 또는 `pool_id` (UniV4/Balancer). `variant_tag` 으로
   subgraph endpoint 와 entity 스키마를 선택.
2. **lookup key 검증**: `venue.chain` 의 숫자부가 `chain_id` param 과 일치하는지 확인 (불일치 →
   에러 → 필드 미방출). 일치하면 `(chain_id, pool_key)` 가 정규 캐시·조회 키.
3. **subgraph 조회 (primary)**: variant→endpoint 매핑으로 GraphQL POST. 풀 entity 에서
   `volumeUSD`(또는 24h 윈도 파생), `totalValueLockedUSD`, `createdAtTimestamp` 를 읽는다.
   응답 키 이름이 DEX 별로 다르므로 `first_path` 식 후보-경로 탐색 권장.
   - `vol24hUsd`: subgraph 가 일별 스냅샷(`poolDayDatas` 등)을 주면 가장 최근 day 의 `volumeUSD`.
     **honest heuristic**: "직전 24h rolling" 이 아니라 "subgraph 의 최근 daily bucket" 에 가까운
     근사일 수 있다. 일부 subgraph 는 누적 `volumeUSD` 만 줘서 24h delta 를 직접 못 뽑는다 — 이
     경우 두 시점(now, now-24h)의 누적값 차분이 필요하거나, 그 풀은 산출 불가로 처리한다.
   - `ageDays`: `floor((now - createdAtTimestamp) / 86400)`.
4. **on-chain TVL fallback (선택, vol24hUsd 산출 불가)**: subgraph 미스 시 V2-류 풀은
   `OnchainViewFetcher` 로 `getReserves()` (또는 token0/token1 `balanceOf(pool)`) multicall →
   각 reserve 를 `oracle.usd_value` (기존 메서드) 로 USD 환산해 합 = `tvlUsd`. 이 경로는
   `vol24hUsd` 를 만들 수 없으므로 그 필드는 미방출. (concentrated/Balancer 풀의 reserve 합산은
   비자명 — fallback 은 constant-product 풀에 한정 권장.)
5. **record 조립 & Decimal 직렬화**: 산출 가능한 필드만 채워 `{ tvlUsd?, vol24hUsd?, ageDays? }`
   반환. **모든 필드를 산출하지 못해도 OK** — 호스트가 부분 record 를 unwrap 하고, manifest 가
   요구하는 `vol24hUsd` 가 있으면 정책이 활성화, 없으면 inert.

**honest limits (반드시 verdict reason 이나 method 문서에 명시)**:
- 24h 거래량은 indexer 의 갱신 주기·정의에 종속 — "정확한 직전 24h" 보장 아님. 경계 부근
  (≈$10k) 풀은 판정이 흔들릴 수 있다. `warn` (deny 아님) 이라 false-warn 의 비용은 모달 1회.
- subgraph 는 인덱싱 지연(re-org/lag)이 있을 수 있고, 신생 풀은 데이터 부재가 흔하다 (→ 미방출).
- multi-asset/weighted/concentrated 풀의 TVL 은 closed-form 이 복잡 — fallback 은 단순 풀 한정.

## on-chain calls

- **Primary 경로 (subgraph)**: **none** — off-chain data-API (GraphQL/REST).
- **TVL fallback 경로 (선택)**: chain = `eip155:<chain_id>` (param `$.root.chain_id` 의 체인).
  - contract = `venue.pool` (constant-product 풀 주소).
  - view fn = `getReserves()` (UniV2: `0x0902f1ac`) 또는 token0/token1 의 `balanceOf(pool)`.
  - **multicall? yes** — `OnchainViewFetcher::fetch_batch` 의 multicall3 `aggregate3`
    (`rpc/multicall.rs`) 로 token0/token1/reserves 를 1 RPC 라운드트립에 묶는다.
  이후 reserve→USD 는 기존 `oracle.usd_value` 핸들러에 위임.

## caching / ttl

- **cache key tuple**: `(chain_id, pool_key)` — 즉 `(eip155 numeric, pool address 또는 pool_id)`.
  variant tag 가 다르면 pool_key 도 다르므로 충돌 없음. `venue.chain` 은 step 2 에서 chain_id 와
  일치 검증되므로 키에 추가 불필요.
- **ttl**: 24h 거래량/TVL 은 분 단위로 천천히 변하고 정책 임계값(`$10k`)이 거칠다 →
  **60–300s** 권장 (예: 120s). `ageDays` 는 사실상 불변 (필요 시 더 길게 분리 캐시 가능).
- **where cached**: 서버 인메모리(LRU 또는 `moka`-류 TTL map), `/v1/rpc` dispatcher 의 메서드
  핸들러 안. (decode-time `live_inputs` sync 레이어와는 별개 — §5 참조.)
- **budget**: 첫 cold 호출은 subgraph HTTP 1회(보통 <1s). HARD_TIMEOUT_MS=8000 의 전체 액션
  예산 안에서, reqwest client timeout 을 보수적(예: rest_json 패턴의 5–10s 미만, 권장 ≤3s)으로
  설정해 단일 메서드가 예산을 독식하지 않게 한다. 캐시 히트는 ~0ms. 한 액션 배치에 여러
  enrichment 가 병렬 dispatch 되므로 개별 핸들러 타임아웃은 8s 보다 충분히 작아야 한다.

## failure & fallback (DORMANCY CONTRACT)

이 메서드는 **optional** (`policy_rpc[].optional: true`, manifest 가 그렇게 선언) 이고
`outputs[].required: false` 다. 따라서 **에러/데이터 부재 시 절대 default 를 대입하지 않는다**:

```
subgraph miss / HTTP fail / unknown venue / vol24hUsd 산출 불가
  ⇒ host 가 vol24hUsd 필드를 result 에 넣지 않음 (또는 call 결과 자체 부재)
  ⇒ host fold 가 그 필드를 drop
  ⇒ context.custom 에 poolVol24hUsd 없음
  ⇒ policy 가드 `context.custom has poolVol24hUsd` = false
  ⇒ forbid 절 미발화 ⇒ 정책 INERT (false verdict 절대 없음)
```

- **NEVER** `vol24hUsd` 에 `0` / `999999999` 같은 sentinel 을 대입하지 말 것. `0` 을 넣으면
  `0 < 10000` 으로 **모든 풀이 거짓 warn** 되고, 큰 값을 넣으면 **저유동성 풀을 놓친다**. 둘 다
  verdict 를 뒤집는 금지 행위.
- **optional 의 degrade 방향**: 메서드 미구현/도달불가/타임아웃 → 누락 입력 → 정책 inert →
  해당 액션은 (이 정책 한에서) **pass** 로 degrade. 절대 hard batch fail 이 아니다. (대조: HL
  venue flow 는 deny-closed 지만 이건 add_liquidity = 온체인 tx flow → warn-closed 세계, 게다가
  optional enrichment 라 누락은 무해.)
- 부분 성공 OK: `vol24hUsd` 만 못 구하고 `tvlUsd`/`ageDays` 는 구해도 됨 — 카탈로그 정책은
  `vol24hUsd` 만 보므로 여전히 inert, 미래 정책은 나머지 둘로 활성화 가능.

## auth / cost / rate-limit

- **API keys (env)**: subgraph 게이트웨이(예: The Graph decentralized network gateway)는 API
  key 를 요구할 수 있다. `rest_json.rs` 의 `RestAuthConfig` 패턴 — `env_var` 로 키를 읽어
  `header_name` 에 주입, 키 부재 시 auth 생략 — 을 그대로 따른다. 키 이름은 구현 시 확정
  (출처 미확인: 정확한 env 변수명은 배포 설정에 종속). public/hosted endpoint 만 쓰면 키 불요.
- **per-call cost**: subgraph GraphQL 쿼리 1건 (게이트웨이는 query 당 과금일 수 있음) 또는
  fallback 시 RPC `eth_call` 1 multicall 라운드. 사실상 read-only, gas 비용 없음.
- **rate-limit**: hosted/decentralized subgraph 는 분당 쿼리 제한이 있다. **캐시가 흡수**:
  `(chain_id, pool_key)` 별 120s TTL 이면 같은 풀에 반복 add_liquidity 가 와도 cold 쿼리는
  TTL 당 1회. 인기 풀일수록 캐시 효율이 높다. 메서드 호출 자체가 `add_liquidity` 액션에만
  트리거되므로 호출 빈도도 낮다.

## activation

이 메서드를 구현(핸들러 작성 + `schema/method-catalog.json` 등록)하면 다음 dormant 카탈로그
정책이 un-dormant 된다:

- `lp-low-liquidity` (`action/swap`, severity `warn`) — trigger `action.tag == "add_liquidity"`,
  guard `context.custom.poolVol24hUsd.lessThan(decimal("10000.0000"))`. (POLICY_RPC_METHODS.md
  활성화 맵의 `pool.liquidity → 1 policy` 항목과 일치.)

다른 메서드와 독립적 — `pool.liquidity` 만 구현해도 이 정책 1개가 즉시 활성화된다.

## primary-source references

- ActionBody `AmmVenue` / `AddLiquidityAction` 정의 (param 형상의 SSOT, 1차=코드):
  `crates/policy-server/asset-model/action/src/amm/mod.rs:90` (`AmmVenue`),
  `.../amm/add_liquidity.rs` (`AddLiquidityAction.venue`).
- 메서드 wire 계약 / 프로젝션 제약 / dormancy 규칙 (1차=in-repo 문서):
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1–3, `#pool.liquidity` 엔트리.
- 재사용 플럼빙 (1차=코드): `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs`
  (REST+auth+pointer 추출), `.../venue/uniswap.rs` (venue HTTP + 후보-경로 파싱),
  `.../onchain.rs` + `.../rpc/multicall.rs` (multicall3 on-chain fallback).
- DEX 분석 subgraph 스키마 (volumeUSD / totalValueLockedUSD / createdAtTimestamp 필드명):
  **출처 미확인** — Uniswap/Curve/Balancer 공식 subgraph 스키마는 구현 시 각 프로토콜 공식
  subgraph repo 의 `schema.graphql` 1차 출처로 확정할 것. 본 spec 은 필드 존재를 가정하지 않고
  "후보-경로 탐색 + 부재 시 미방출" 로 방어한다.
- The Graph 게이트웨이 인증/요금 모델: **출처 미확인** — 배포 시 The Graph 공식 docs 로 확정.
