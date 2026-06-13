# method: oracle.usd_value

status: existing (in method-catalog.json)

> 이 파일은 미래의 `/v1/rpc` 서버 구현자가 읽는 **구현 스펙**이다. wire interface 만이 아니라
> *어떻게* 만드는지를 기술한다. wire 계약/projection 제약/활성화 정책 목록의 1차 출처는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§2, §3a, §4) 이며,
> 재사용 가능한 plumbing 의 1차 출처는 `crates/policy-server/sync/src/sources/fetchers/{oracle,onchain,rpc}` 코드다.
> 모든 진술은 해당 코드/문서에 grounding 되어 있고, 검증 못 한 것은 "출처 미확인" 으로 명시했다.

## purpose

한 토큰의 **on-chain 명목 수량(uint256 wei-form)** 을 **USD 명목가치(Decimal)** 로 환산한다.
Dambi 의 USD-cap 류 정책 — "X 달러 넘는 approve / swap / transfer / borrow / perp 포지션은
warn/fail" — 은 토큰 단위가 아니라 **달러 단위**로 한도를 건다. 그런데 calldata 만으로는
`amount * price / 10^decimals` 를 계산할 가격(price)을 알 수 없다 (정적 디코드는 가격 oracle 을
모른다). 따라서 이 enrichment 가 가격을 가져와 USD 로 환산해 `context.custom.<field>` (예:
`usdValue`) 로 주입하고, 정책이 그 Decimal 필드에 한도를 비교한다. Dambi 의 no-simulation 모델과
일관되게 이것은 **순수 fetch(+산술)** 이지 트랜잭션 시뮬레이션이 아니다.

## interface

1차 출처: `schema/method-catalog.json` 의 `oracle.usd_value` 엔트리 + `POLICY_RPC_METHODS.md` §3a.

### params (각 `$.`-selector + type)

| param | type | required | defaultSelector | 설명 |
|---|---|---|---|---|
| `chain_id` | `Long` | yes | `$.root.chain_id` | EIP-155 chain id (예: `1` = ethereum). |
| `asset` | `AssetRef` | yes | `$.action.inputToken.asset` | 가격을 매길 토큰. address/symbol/decimals 를 담은 ref. |
| `amount` | `String` | yes | `$.action.inputToken.amount.value` | on-chain raw amount (uint256 wei-form 10진 문자열). |
| `source` | `String` | no | — | `coingecko`(default) \| `chainlink`. enum. CoinGecko 는 HTTP-only 라 바로 동작; chainlink 는 on-chain feed 라 chain 당 RPC 설정 필요. |

> **non-swap caller selector note** (`POLICY_RPC_METHODS.md` §3a):
> catalog `defaultSelector` 는 swap-shaped (`$.action.inputToken.*`) 이지만, approve/transfer/borrow/perp
> caller 는 각 action 자기 모양으로 override 한다 — `$.action.token` / `$.action.asset` /
> `$.action.collateralToken` (asset) + `$.action.amount` / `$.action.collateralAmount` (amount).
> 이 `$.action.*` selector 는 **ActionView** JSON 에 대해 resolve 되며, 그 spelling 의 authority 는
> `/v1/rpc` planner 다. 구현자는 manifest 가 넘기는 selector 를 신뢰하고, 자기 param 타입만 맞추면 된다.

### result shape (record fields + types)

`method-catalog.json` 의 `returns` 는 `{ "kind": "record", "type": "UsdValuation" }`. 최소 필드:

| field | type | 설명 |
|---|---|---|
| `usd` | Decimal (문자열) | **필수.** 환산 결과 USD 명목가치. 정책이 읽는 유일한 leaf. |
| `price` | Decimal (문자열) | optional. 단가 (1 토큰당 USD). debug/reason 용. |
| `source` | String | optional. 실제 사용한 source (`coingecko`\|`chainlink`). |
| `priceTs` | Long | optional. 가격 timestamp (chainlink `updatedAt`; coingecko 면 fetch 시각). staleness 판단용. |
| `decimals` | Long | optional. 환산에 쓴 토큰 decimals. |

> `usd` 외 필드는 전부 부가정보다. v2 엔진은 record 를 통째로 받지 못하므로(아래 projection 참고)
> 이 부가 필드들은 host 가 채우든 말든 정책 verdict 에 영향이 없다 — 구현 1차 컷은 `usd` 만 채워도 된다.

### projection: `$.result.usd → Decimal` (record→scalar leaf — **mandatory**)

`POLICY_RPC_METHODS.md` §2 의 hard 제약: `materialize_v2` 는 **scalar** projection type 만
`context.custom.*` 에 받는다 — `String | Long | Bool | Decimal | Set<String>`. legacy record type
(`UsdValuation`, `WindowStats`) 은 제거됐다. 따라서 record 를 반환하더라도 manifest 가
`outputs[].from = "$.result.usd"`, `outputs[].type = "Decimal"` 로 **leaf scalar 까지 투영**해야 한다.
`outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 가 강제). `custom_context`
철자는 lowercase Cedar (`"decimal"`). 즉 host 가 어떤 모양으로 돌려주든, 정책이 실제로 보는 것은
`context.custom.usdValue : decimal` 하나다.

## data source(s)

`source` param 으로 분기. 둘 다 **decode-time enrichment 레이어에 EXISTING-FETCHER 로 존재** —
`/v1/rpc` dispatcher 는 새로 짜되, 안쪽 fetch plumbing 은 그대로 재사용한다.

### `source = "coingecko"` (default) — EXISTING-FETCHER-REUSABLE

- **Fetcher**: `RestJsonOracleFetcher`
  (`crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs`).
- 무엇을 주는가: base_url + per-`feed_id` `{path, json_pointer}` 로 HTTP GET → 응답 JSON 에서
  `json_pointer` 로 단가 추출 → `Decimal`. env-var 기반 auth header(`RestAuthConfig{header_name, env_var}`)
  를 build-time 에 resolve. timeout 은 `RestOracleConfig.timeout_sec`.
- 재사용 방식: CoinGecko `/simple/price?ids=<id>&vs_currencies=usd` path + json_pointer
  `/<coingecko-id>/usd` 를 feed 로 등록하면 단가를 그대로 얻는다 (코드 테스트의 `cfg_no_auth()` 가
  정확히 이 path 모양을 쓴다). **NET-NEW 인 부분**: (a) `AssetRef`(chain+address/symbol) →
  CoinGecko coin-id mapping (예: contract-address 기반 lookup), (b) wei-form `amount` × 단가 ÷
  10^decimals 산술. fetcher 자체는 단가만 준다.

### `source = "chainlink"` — EXISTING-FETCHER-REUSABLE

- **Fetcher**: `ChainlinkFetcher`
  (`crates/policy-server/sync/src/sources/fetchers/oracle/chainlink.rs`).
- 무엇을 주는가: `(chain, feed_id)` registry 로 feed 주소/decimals 를 찾고,
  `RpcRouter::eth_call` 로 `latestRoundData()` 호출 → returndata 의 `answer`(2번째 word) 를
  feed decimals 로 scale 해 `Decimal` 반환 (`scale_to_decimal`). 음수/패딩 처리 포함.
- 재사용 방식: `ChainlinkFeedRegistry::from_config(&ChainlinkConfig)` 로 chain 별 feed 등록 →
  `ChainlinkFetcher::from_sync_config(router, cfg)`. `with_mainnet_defaults()` 는 USDC/USDT/ETH/WBTC/DAI
  /USD 8-decimals feed 주소를 이미 박아둔 test helper (참조용 ground-truth 주소). **NET-NEW 인 부분**:
  (a) `AssetRef` → `feed_id`("WBTC/USD" 등) mapping, (b) 같은 wei×price÷10^decimals 산술,
  (c) chain 당 RPC 설정(`POLICY_RPC_CHAIN_RPCS`, catalog 설명 인용).

### NET-NEW (양쪽 공통)

- `/v1/rpc` method dispatcher 자체 (오늘 repo 에 **없음** — `POLICY_RPC_METHODS.md` §5).
  `policy-server` `handler.rs` 의 `results = BTreeMap::new()` 는 `/evaluate` 시뮬레이션 경로지
  이 enrichment endpoint 가 아니다.
- `AssetRef` → (coingecko coin-id | chainlink feed_id) 해소 테이블.
- 토큰 `decimals` 확보: `AssetRef` 에 decimals 가 실려오면 그걸 쓰고, 없으면 ERC-20 `decimals()`
  on-chain read — 이때 `OnchainViewFetcher` / `Multicall.aggregate3`
  (`fetchers/onchain.rs`, `fetchers/rpc/multicall.rs`) 를 재사용 (`balanceOf`/`decimals` 셀렉터 인코딩
  precedent 이 그 파일 테스트에 있다).

## derivation algorithm

입력: `chain_id` (Long), `asset` (AssetRef), `amount` (uint256 10진 문자열), `source` (default `coingecko`).

1. **decimals 확정.** `asset.decimals` 가 있으면 그 값 `d`. 없으면 `(chain_id, asset.address)` 로
   ERC-20 `decimals()` on-chain read (`OnchainViewFetcher`, 또는 다른 enrichment 와 batch 시
   `Multicall.aggregate3`). 둘 다 실패 → **abort** (이 호출은 결과 없이 끝남 → dormancy contract, 아래).
2. **단가(price, 1 토큰당 USD) 확보.**
   - `source = "chainlink"`: `(chain_id, asset)` → `feed_id` → `ChainlinkFetcher::fetch_price` →
     `Decimal` 단가. `updatedAt` 을 `priceTs` 로 보존하면 staleness 표면화에 쓸 수 있다.
   - `source = "coingecko"`: `asset` → coingecko coin-id → `RestJsonOracleFetcher::fetch_price` →
     `Decimal` 단가. fetch 시각을 `priceTs` 로.
   - 단가를 못 얻으면 **abort** (결과 없음).
3. **환산.** `usd = (amount / 10^d) * price`. `amount` 는 정수 문자열, `price`·`usd` 는 `Decimal`
   (문자열). 정밀도: 분자 `amount * price_numerator` 를 정수로 곱한 뒤 `10^d` 와 price 의 소수
   스케일로 나눠 십진 절단/반올림. **부동소수 금지** — `Decimal` 의 십진 산술을 쓴다
   (chainlink fetcher 의 `scale_to_decimal` 가 같은 십진-문자열 스케일링 패턴을 보여준다).
4. **record 조립.** `{ usd, price, source, priceTs?, decimals: d }`. host fold 는 이 record 를
   `map[call_id]` 로 넣고, manifest projection `$.result.usd → Decimal` 이 `usdValue` leaf 를 뽑는다.

### heuristic limits (정직한 한계)

- **price 출처 = spot 단가 1점.** flash-loan / sandwich 로 한 블록 동안 왜곡된 시세, depeg, low-liquidity
  토큰의 비현실적 단가를 보정하지 않는다. USD-cap 은 "정상 시세 기준 대략적 한도" 이지 정밀 평가가 아니다.
- **staleness.** chainlink `updatedAt` 또는 coingecko fetch 시각이 오래됐어도 이 메서드는 *값을 준다*.
  staleness 차단을 원하면 별도 정책/필드(`priceTs`)로 표면화해야 하지, 여기서 임의 default 로 막지 않는다.
- **decimals fallback.** `asset.decimals` 부재 시 on-chain `decimals()` 에 의존 — non-standard 토큰
  (rebasing/proxy) 은 decimals 가 의미와 어긋날 수 있다. 그 류는 `token.metadata` 메서드의 영역.
- **AssetRef→id mapping 누락.** mapping 에 없는 토큰은 단가를 못 얻어 결과가 비고(abort) → 정책 INERT.
  과장 금지: 매핑 커버리지가 곧 이 메서드의 커버리지다.

## on-chain calls

- `source = "chainlink"`: **있음.** chain = `eip155:<chain_id param>`; contract = feed 주소
  (registry lookup); view fn = `latestRoundData()` (selector = `keccak("latestRoundData()")[..4]`,
  `ChainlinkFetcher` 가 `RpcRouter::eth_call` 로 호출); 결과 word2(`answer`) 를 feed decimals 로 scale.
- decimals fallback (양쪽 source 공통, `asset.decimals` 부재 시): ERC-20 `decimals()`
  (selector `0x313ce567`) read.
- **multicall?**: 단독 호출은 단건 `eth_call`. 동일 action 에서 다른 on-chain enrichment 와 같은 chain·
  같은 batch 로 묶일 때만 `Multicall.aggregate3` (`fetchers/onchain.rs::fetch_batch`) 로 합치는 게 권장.
- `source = "coingecko"`: **none (off-chain / data-API)** — decimals fallback 이 필요한 경우를 빼면 RPC 0회.

## caching / ttl

- **price cache key**: `(chain_id, source, feed_id|coin-id)`. value = `(price: Decimal, priceTs)`.
  단가는 토큰별로 공유되므로 amount 와 무관하게 캐시한다 (환산은 캐시된 단가에 산술만).
- **decimals cache key**: `(chain_id, asset.address)` → `d` (사실상 불변, 장수명 캐시 가능).
- **ttl**: 단가 TTL 짧게 — 권장 30s~60s (시세는 휘발성; staleness 와 latency 사이 trade-off).
  decimals 는 길게(시간~영구). **출처 미확인** (이 TTL 수치는 본 repo 코드에 명시 상수가 없는 권장값).
- **위치**: `/v1/rpc` 서버 in-process 캐시. CoinGecko REST·Chainlink RPC 왕복은 수십~수백 ms 이고,
  per-action 배치는 `HARD_TIMEOUT_MS = 8000` (orchestrator) 예산 안에 들어야 한다 — cache hit 이면 산술뿐,
  cold miss 라도 단일 HTTP/RPC 1왕복은 예산 내. 같은 토큰의 cap 정책 여러 개가 한 action 에서 동시
  활성이면 cache 가 단가 1회로 흡수한다.

## failure & fallback (DORMANCY CONTRACT)

이 메서드의 모든 catalog caller 는 `policy_rpc[].optional: true` (catalog enrichment 의 표준).
따라서:

- price 또는 decimals 확보 실패, source mapping 누락, RPC/HTTP 에러, timeout, param selector 미해소 →
  **결과 record 에서 `usd` 필드를 내보내지 않는다** (또는 result 를 통째로 비운다).
- host fold 는 missing/`ok:false` 결과를 `map[call_id]` 에서 **drop** 한다
  (`POLICY_RPC_METHODS.md` §1 wire contract).
- 그 결과 `context.custom` 에 `usdValue` 필드가 **없음** → 정책의 `context.custom has usdValue` guard 가
  **false** → 해당 cap 정책은 **INERT** (verdict 미생성). dormant 정책은 false verdict 를 만들지 않는다.
- **절대** verdict 를 뒤집을 수 있는 default(예: `usd = 0` 이나 `usd = ∞`) 를 대입하지 않는다.
  `0` 이면 cap 우회(false pass), 큰 값이면 false fail — 둘 다 금지. `optional: true` 이므로 missing
  input 은 batch hard-fail 이 아니라 **pass 로 degrade**(해당 정책만 inert)한다.
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. fail-closed 방향(deny)
  으로 흐르지 않는다. (이는 enrichment 의 일반 계약이고, deny-closed 인 HyperLiquid 경로와 무관 —
  이 메서드는 cap 정책 enrichment 일 뿐 venue 차단 로직이 아니다.)

## auth / cost / rate-limit

- **CoinGecko**: API key 는 `RestAuthConfig{header_name, env_var}` 로 build-time 에 env 에서 resolve
  (`RestJsonOracleFetcher::from_sync_config`). env 없으면 auth header 생략(무인증 free tier 로 fallback).
  - per-call cost: HTTP GET 1회/토큰(캐시 miss 시).
  - rate-limit: CoinGecko free tier 는 분당 호출 제한이 있다(구체 수치 **출처 미확인** — 플랜 가격 정책에
    따라 변동). key 있는 유료 플랜이면 한도 상향.
  - 캐시 흡수: 단가 캐시(TTL 30~60s)가 동일 토큰 반복 호출을 1회로 접어 rate-limit 압력을 크게 줄인다.
- **Chainlink**: API key 불필요(public RPC), 단 chain 당 RPC endpoint 설정 필요
  (`POLICY_RPC_CHAIN_RPCS`, catalog `source` 설명 인용). RPC provider 의 자체 rate-limit 적용.
  - per-call cost: `eth_call` 1회/토큰(+decimals fallback 시 1회 더; multicall 로 1회로 합칠 수 있음).
  - 캐시 흡수: price·decimals 캐시가 RPC 왕복을 토큰별 1회로 접는다.

## activation

이 메서드를 구현하면 다음 5개 catalog 정책이 dormant 에서 해제된다
(`POLICY_RPC_METHODS.md` §4 activation map):

- `large-erc20-approve` (wallet/usd-cap)
- `swap-usd-cap` (wallet/usd-cap)
- `transfer-usd-cap` (wallet/usd-cap)
- `borrow-usd-cap` (wallet/usd-cap)
- `perp-position-usd-cap` (wallet/usd-cap)

`POLICY_RPC_METHODS.md` 의 "minimal first cut" 권고: `oracle.usd_value`(5) + `clock.now`(2) 가 가장 많은
정책을 활성화하며 둘 다 이미 catalog 에 있어 `/v1/rpc` dispatcher 본체만 만들면 된다.

## primary-source references

- Dambi enrichment wire 계약 / projection 제약 / activation map:
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3a, §4, §5) — repo 내부 1차.
- 메서드 카탈로그 엔트리(params/returns/source enum/origin): `schema/method-catalog.json` `oracle.usd_value`.
- 재사용 fetcher (1차 = 코드):
  - `crates/policy-server/sync/src/sources/fetchers/oracle/rest_json.rs` (`RestJsonOracleFetcher` — CoinGecko REST).
  - `crates/policy-server/sync/src/sources/fetchers/oracle/chainlink.rs` (`ChainlinkFetcher` / `ChainlinkFeedRegistry` / `scale_to_decimal`).
  - `crates/policy-server/sync/src/sources/fetchers/oracle/mod.rs` (`PriceFetcher` trait, `provider_key`).
  - `crates/policy-server/sync/src/sources/fetchers/onchain.rs` (`OnchainViewFetcher` / `fetch_batch` — ERC-20 decimals fallback).
  - `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs` (`Multicall::aggregate3`).
- 외부 데이터 소스 공식 docs:
  - CoinGecko API — https://docs.coingecko.com/ (Simple Price / contract-address lookup 엔드포인트). free-tier rate-limit 정확 수치는 **출처 미확인**(플랜 의존).
  - Chainlink Price Feeds — https://docs.chain.link/data-feeds/price-feeds (`AggregatorV3Interface.latestRoundData()`, feed 주소/decimals).
  - EIP-155 (chain id) — https://eips.ethereum.org/EIPS/eip-155.
  - ERC-20 `decimals()` — https://eips.ethereum.org/EIPS/eip-20.
