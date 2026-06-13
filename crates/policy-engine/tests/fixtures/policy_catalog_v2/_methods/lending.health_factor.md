# method: lending.health_factor

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> Implementer note: this file is the **HOW-to-build** spec. The wire interface (params, result
> shape, projection) is the contract in
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §3 `lending.health_factor`;
> this doc adds the data source, derivation, on-chain plumbing, caching, and dormancy contract.
> Two catalog manifests already wire this method:
> `action/lending/borrow-low-health-factor/manifest.json` and
> `action/lending/withdraw-collateral-health-factor/manifest.json`. Build to **their** params/outputs,
> not to a guessed shape.

## purpose

대출 포지션의 **action 이후 projected health factor (HF)** 를 계산해, 청산 근접도를 정책이
판단할 수 있게 한다. 사용자가 추가 borrow 하거나 collateral 을 withdraw 하면 HF 가 떨어지고,
HF < 1 이면 청산 대상이 된다. Dambi 은 트랜잭션을 **시뮬레이션하지 않으므로**, 이 method 는
프로토콜의 현재 user account 상태를 읽어(현 collateral / debt / liquidation threshold) action
파라미터(asset + amount)를 **closed-form** 으로 반영한 post-action HF 를 재계산한다. 이 값이
catalog 정책 `borrow-low-health-factor` / `withdraw-collateral-health-factor` 의 `< 1.5` warn
임계와 비교된다. 핵심 한계: 이것은 *추정(estimate)* 이지 트랜잭션 실행 결과가 아니다 — 가격
oracle 지연, 같은 batch 내 다른 leg, 멀티-reserve 상호작용은 반영하지 못한다 (아래 §derivation
의 heuristic limits 참고).

## interface

**Params** (manifest `policy_rpc[].params`; 모두 `$.`-selector 로 해석, dot-notation only):

| param | type | default selector (from manifest) | note |
|---|---|---|---|
| `chain_id` | `Long` | `$.root.chain_id` | EIP-155 chain id; `eip155:<chain_id>` 로 RPC 라우팅 |
| `owner` | `String` | `$.root.from` | 포지션 소유자 (account-data view 의 user 인자) |
| `venue` | (Venue) | `$.action.venue` | 어느 lending 프로토콜인지 — Pool 주소 resolve 키 |
| `asset` | `AssetRef` | `$.action.asset` | borrow/withdraw 대상 토큰 (address + decimals + symbol) |
| `amount` | `String` | `$.action.amount` | on-chain uint256 wei-form 문자열 (해당 asset 의 단위) |

> 출처: 두 manifest 의 `policy_rpc[0].params` 블록과 동일. `optional: true` (둘 다) — selector
> 하나라도 결측이면 call 을 **skip** 한다 (fail 아님). dormancy-friendly default.

**Result shape** (record):

```
{
  postActionHf: Decimal,   // action 반영 후 추정 HF (예: "1.4231"); 무한대(부채 0)는 클램프 — §derivation 6
  currentHf?:   Decimal,   // action 전 현재 HF (참고/로그용, optional)
  ltv?:         Long       // 현재 loan-to-value, bps (Aave currentLiquidationThreshold/ltv 계열)
}
```

**Projection** (record → scalar leaf — **필수**: v2 에는 record ProjectionType 이 없으므로 manifest
output 은 반드시 leaf 를 가리켜야 한다):

- primary: `$.result.postActionHf -> Decimal`  (두 manifest 의 `outputs[0].from`, `field: "postActionHf"`)
- 보조(원하면 두 번째 output 으로): `$.result.ltv -> Long`

두 manifest 모두 `outputs: [{ kind:"context", field:"postActionHf", type:"Decimal",
from:"$.result.postActionHf", required:false }]` 이고 `custom_context.fields.postActionHf = "decimal"`.
즉 host fold 후 `context.custom.postActionHf` (Decimal) 한 필드만 정책에 노출된다.

## data source(s)

**EXISTING-FETCHER-REUSABLE.** 현재 account-data read 에 필요한 plumbing 은 이미 존재한다:

- **On-chain view fetcher** — `crates/policy-server/sync/src/sources/fetchers/onchain.rs`
  (`OnchainViewFetcher::fetch_one` / `fetch_batch`). `DataSource::OnchainView { chain, contract,
  function, decoder_id }` 를 받아 selector+args 인코딩 → `eth_call` → decoder 적용. 그대로 재사용.
- **Aave V3 account-data decoder** — *이미 등록됨*. 두 군데 중 하나 사용:
  - hand-coded: `decoder.rs:138` `decode_aave_user_data` (id `"aave_user_data"`, 등록 `decoder.rs:62`)
    → `{ totalCollateralBase, totalDebtBase, availableBorrowsBase, currentLiquidationThreshold,
    ltv, healthFactor }` 전부 uint256 decimal-string.
  - dyn-abi: `abi_decoder/types.rs:46` id `"aave_v3_user_account_data"`
    (`(uint256,uint256,uint256,uint256,uint256,uint256)`).
  `getUserAccountData(address)` returns 6×uint256 = `(totalCollateralBase, totalDebtBase,
  availableBorrowsBase, currentLiquidationThreshold, ltv, healthFactor)`. **healthFactor 는 1e18
  스케일**, `currentLiquidationThreshold`/`ltv` 는 bps. base 통화는 Aave price oracle 단위(USD, 8
  decimals).
- **Multicall** — `rpc/multicall.rs` `Multicall::aggregate3` (Multicall3, selector `0x82ad56cb`).
  여러 read 를 한 `eth_call` 로 묶을 때 재사용 (§on-chain calls).
- **RPC router / provider failover** — `rpc/router.rs` (`eth_call`, `multicall_addr`), 체인별
  provider 는 `rpc/config.rs` `RpcConfig` (env-expanded TOML, `POLICY_RPC_CHAIN_RPCS` 류).

**NET-NEW plumbing** (구현 시 추가해야 하는 것):

1. **`/v1/rpc` method dispatcher 자체** — POLICY_RPC_METHODS.md §5 명시대로 in-repo 부재.
   `method` 키로 분기해 `call_id` 별 `results` 를 채우는 핸들러 레지스트리. (이 method 만이 아니라
   6개 new method 공통 선결 작업.)
2. **venue → Pool 주소 resolver** — `$.action.venue` (예: Aave V3) → 체인별 `Pool` (또는
   `PoolDataProvider`) 주소 매핑. 현재 venue fetcher(`venue/`)는 HL/Uniswap 전용이라 lending Pool
   주소표가 없다. 정적 allowlist(체인×프로토콜→주소) 로 시작 권장 — §primary-source 참고.
3. **non-Aave 프로토콜 어댑터** — Compound v3 / Morpho / Spark 등은 `getUserAccountData` 형태가
   아니므로 별도 account-read + HF 공식이 필요. **1차 구현은 Aave V3 (및 Spark = Aave V3 fork) 만**
   지원하고 나머지 venue 는 §failure 대로 **필드 미방출(dormant)** 로 둔다 — 추측 금지.

## derivation algorithm

목표: `getUserAccountData` 의 현재 상태 + action(asset, amount, borrow|withdraw)을 반영한
post-action HF. Aave 정의: `HF = (totalCollateralBase × liquidationThreshold) / totalDebtBase`
(threshold 는 가중평균, bps). 1e18 스케일.

1. **trigger 분기 결정** — manifest trigger 로 이미 알 수 있음:
   `borrow-low-health-factor` → `action.tag == "borrow"`, `withdraw-collateral-health-factor` →
   `action.tag == "withdraw"`. (params 에 tag 가 없으면 dispatcher 가 manifest id 또는 별도
   `$.action.tag` selector 로 구분 — 구현 시 params 에 `tag` 를 추가하거나 두 method-id 로 분리.)
2. **venue → Pool 주소 resolve** (§data NET-NEW 2). 미지원 venue → **abort, 필드 미방출**.
3. **현재 account data read** — `Pool.getUserAccountData(owner)` (eip155:`chain_id`).
   `totalCollateralBase (C0)`, `totalDebtBase (D0)`, `currentLiquidationThreshold (LT, bps)`,
   `ltv`, `healthFactor (HF0, 1e18)` 추출. `currentHf = HF0 / 1e18`, `ltv` 그대로 result 로.
4. **action 의 base-통화 환산 (Δ)** — `amount` (asset 단위) 를 Aave base 통화로 환산해야 한다.
   `getUserAccountData` 는 토큰별 내역을 주지 않으므로 두 경로:
   - (정확) `Pool.getReserveData(asset)` 의 price 또는 Aave `AaveOracle.getAssetPrice(asset)` 로
     `amount × price / 10^assetDecimals` = ΔBase. 이 view 들도 같은 `OnchainViewFetcher` +
     decoder(`aave_v3_reserve_data` 이미 등록) 로 읽을 수 있다 → multicall 로 묶음.
   - (heuristic, oracle 미배선 시) `oracle.usd_value` enrichment 결과나 외부 price 로 ΔUSD ≈ ΔBase
     (Aave base = USD 8-dec). 정밀도 한계 명시.
5. **post-action 상태 계산** (closed-form, 같은 base 단위):
   - **borrow**: 부채만 증가. `D1 = D0 + ΔBase`, `C1 = C0`, `LT 불변` (신규 borrow 가 collateral
     mix 를 안 바꾸므로 가중 LT 근사 유지).
     `HF1 = (C1 × LT_frac) / D1`, where `LT_frac = LT_bps / 10000`.
   - **withdraw**: collateral 만 감소. `C1 = C0 − ΔBase`, `D1 = D0`.
     `HF1 = (C1 × LT_frac) / D1`. (`C1 < 0` 이면 0 으로 클램프 → HF1 = 0, 강한 warn.)
     주의: 단일 reserve 를 빼면 가중 LT 가 바뀔 수 있으나, account-data 만으로는 per-reserve LT 를
     모른다 → `LT_frac` 를 현 가중값으로 **고정**하는 것이 heuristic. (보수적으로 약간
     낙관적일 수 있음 — 명시.)
6. **클램프 & 포맷** — `D1 == 0` (부채 0): HF 는 수학적으로 ∞ → 정책상 안전, `postActionHf` 를
   큰 상수(예: `"999999.0000"`)로 클램프하거나 **필드를 방출하되 임계 위**로 둔다 (warn 안 뜸).
   Decimal 문자열은 4-dp (정책이 `decimal("1.5000")` 비교 → 동일 scale 권장).
   `currentHf` 동일 포맷.

**Heuristic limits (정직하게):**
- 트랜잭션 시뮬레이션이 아님 — 가격은 read 시점 oracle 값, action 직전/직후 가격 변동 미반영.
- 같은 batch/multicall 의 **다른 leg** (예: 동시 supply + borrow) 미반영 — 단일 action 기준.
- 가중 `LT_frac` 고정 가정 (per-reserve LT 부재) → multi-collateral 포지션의 withdraw 는 실제와
  차이 가능. borrow 는 비교적 정확.
- Aave base-통화 환산이 reserve price read 없이 외부 USD 근사로 떨어지면 ΔBase 오차 누적.
- **이 모든 경우에도 §failure 의 dormancy contract 가 안전판** — 불확실하면 필드를 안 내보내
  정책을 inert 로 만든다 (틀린 HF 로 warn/pass 를 뒤집지 않음).

## on-chain calls

`venue == Aave V3` (or Spark) 기준:

| call | chain | contract | view fn | decoder_id | multicall? |
|---|---|---|---|---|---|
| account data | `eip155:<chain_id>` (param `chain_id`) | venue Pool (resolve, §data NET-NEW 2) | `getUserAccountData(address owner)` → 6×uint256 | `aave_user_data` / `aave_v3_user_account_data` | leg 1 |
| asset price (정확 환산용) | 동 chain | `AaveOracle` (또는 `Pool.getReserveData(asset)`) | `getAssetPrice(address)` / `getReserveData(address)` | u256 / `aave_v3_reserve_data` | leg 2 |

→ 두 read 를 **Multicall3 `aggregate3`** (`rpc/multicall.rs`, `allow_failure: true`) 한 번으로 묶어
RTT 1회. Multicall3 주소는 `RpcRouter::multicall_addr(chain)` (config). price read 가 실패(leg 2
`!success`)하면 §derivation 4 heuristic 경로로 degrade, 그래도 안 되면 §failure.

> non-Aave venue: **none (미지원 → 필드 미방출)**. 추후 어댑터 추가 시 이 표를 프로토콜별로 확장.

## caching / ttl

HF 는 사용자 포지션·가격에 따라 자주 변하므로 **짧은 TTL**.

- **account-data cache key**: `(chain_id, pool_addr, owner)` → `(C0, D0, LT, HF0)`, **TTL 5–10s**.
  (같은 `owner` 의 borrow/withdraw 두 정책이 같은 action 에서 동시 평가 → 한 read 공유.)
- **asset-price cache key**: `(chain_id, asset_addr)` → base-price, **TTL 15–30s** (가격은 read
  공유 빈도 높음).
- **venue→Pool 주소**: 정적 → **무기한(process lifetime)** 캐시.
- 저장 위치: `/v1/rpc` 서버 in-process LRU/TTL 맵 (decode-time fetcher 들과 동일 패턴). per-action
  results 맵과 **혼동 금지** — host 는 action 마다 fresh results 맵을 받지만(같은 `call_id` 가
  action 간 반복), 이 데이터 캐시는 서버측 read 캐시로 독립.
- **budget**: `HARD_TIMEOUT_MS = 8000` (orchestrator). multicall 1 RTT (~수백 ms) + 캐시 히트시
  ~0. 캐시 미스라도 단일 aggregate3 한 번이라 여유. 타임아웃 임박 시 §failure 로 필드 drop.

## failure & fallback (DORMANCY CONTRACT)

**핵심 불변식: 불확실하면 필드를 내보내지 않는다.** 어떤 단계든 실패하면 (selector 결측 / 미지원
venue / Pool 주소 unresolved / `eth_call` revert / decode 실패 / 타임아웃) **`postActionHf` 필드를
방출하지 않는다**:

```
no postActionHf field
  ⇒ host fold 가 해당 output 을 drop
  ⇒ context.custom 에 postActionHf 부재
  ⇒ 정책의 `context.custom has postActionHf` guard 가 false
  ⇒ 정책 INERT (forbid 절 미발화 → false verdict 없음)
```

- 정책 cedar 가 `context has custom && context.custom has postActionHf && ... lessThan(1.5)` 3중
  guard 라 필드 부재 = 안전하게 무력화 (`borrow-low-health-factor` / `withdraw-collateral-health-factor`
  policy.cedar 동일).
- **절대 금지**: 결측 시 default HF (예: `"2.0000"` 또는 `"0.0000"`) 치환 — verdict 를 뒤집을 수
  있음. 모르면 침묵.
- manifest `optional: true` + output `required: false` → 결측 param/필드는 **이 call 을 skip** 하고
  배치를 hard-fail 시키지 않는다. 한 enrichment leg 실패가 다른 정책/leg 으로 번지지 않게
  **degrade-to-pass** (이 method 가 없으면 그 정책만 dormant, 액션은 정상 평가).
- 부분 성공(account-data OK, price 환산 실패): §derivation 4 heuristic 시도 → 그것도 신뢰 못하면
  필드 미방출. **추정 HF 를 추측으로 채우지 말 것.**

## auth / cost / rate-limit

- **RPC provider**: `eth_call` (account-data + price) 는 체인 RPC. `RpcConfig` 의 provider 가
  Alchemy/Infura 류면 API 키 env (`${...}` expand, `rpc/config.rs`); publicnode 등 keyless 도 가능.
  키는 `POLICY_RPC_CHAIN_RPCS` 계열 env (oracle.usd_value `source:chainlink` 와 동일 요구).
- **per-call cost**: multicall 로 묶으면 action 당 `eth_call` **1회**. 두 정책(borrow/withdraw)이
  같은 action 에 동시 트리거돼도 account-data cache 공유로 read 1회.
- **rate-limit**: 캐시 TTL(5–10s account / 15–30s price)이 동일 user/asset 반복을 흡수 →
  버스트시 provider 호출 급증 방지. 정적 venue→Pool 캐시는 호출 0.
- 외부 price API(coingecko 등)로 환산 시 그 API 의 rate-limit·키 정책을 따르되, on-chain
  `AaveOracle` 경로를 우선(추가 키 불요, 캐시 가능)으로 권장.

## activation

이 method 를 구현하면 다음 catalog 정책이 dormant 에서 해제된다:

- **`borrow-low-health-factor`** (`action/lending`, trigger `action.tag == "borrow"`, warn,
  `postActionHf < 1.5`).
- **`withdraw-collateral-health-factor`** (`action/lending`, trigger `action.tag == "withdraw"`,
  warn, `postActionHf < 1.5`).

(POLICY_RPC_METHODS.md §4 activation map: `lending.health_factor` = **new**, 2 policies.)
구현 = `schema/method-catalog.json` 에 이 method 등록(현재 미등재) + `/v1/rpc` dispatcher 핸들러
추가 + (선택) `browser-extension/public/method-catalog.json` 동기화.

## primary-source references

- **Aave V3 Protocol — `Pool.getUserAccountData`** (returns `totalCollateralBase, totalDebtBase,
  availableBorrowsBase, currentLiquidationThreshold, ltv, healthFactor`; healthFactor 1e18 스케일,
  base = oracle 통화 8-dec). Aave V3 공식 문서/`aave-v3-core` 컨트랙트. (in-repo decoder 가
  이 6-tuple 을 그대로 디코드: `decoder.rs:135-158`, `abi_decoder/types.rs:42-50`.)
- **Aave V3 `AaveOracle.getAssetPrice` / `Pool.getReserveData`** — asset → base price 환산용
  view. Aave V3 공식 문서.
- **Multicall3 `aggregate3`** — selector `0x82ad56cb`, canonical 주소
  `0xcA11bde05977b3631167028862bE2a173976CA11` (대부분 체인 동일). 공식 multicall3 배포 문서.
  (in-repo: `rpc/multicall.rs`, `config.rs` 테스트 fixture 에 동 주소.)
- **Health factor / liquidation 정의** — `HF = Σ(collateral_i × liqThreshold_i) / totalDebt`,
  HF < 1 청산. Aave V3 risk 파라미터 문서.
- venue→Pool 주소 매핑의 정확한 체인별 주소 테이블: **출처 미확인** (구현 시 Aave
  address-book / 공식 deployments 에서 확정 필요 — 이 spec 에 하드코딩하지 않음).
- 이 method 의 **wire 계약**(params/result/projection/activation)의 1차 근거는 두 manifest
  (`borrow-low-health-factor`, `withdraw-collateral-health-factor`) + `POLICY_RPC_METHODS.md`
  §3 `lending.health_factor`.
