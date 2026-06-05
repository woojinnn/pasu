# method: token.metadata

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> 이 파일은 `/v1/rpc` 서버 구현자가 읽는 **HOW-to-build** 스펙이다. 와이어 인터페이스
> (`POLICY_RPC_METHODS.md` §3c `token.metadata`) 를 전제로, 그 메서드를 **어떻게 구현하는지**
> (데이터 소스 / 재사용 가능한 fetcher / 파생 알고리즘 / 캐싱 / 실패 시 dormancy) 를 명시한다.
> 모든 사실 진술은 코드 또는 1차 출처 기반이며, 미검증 항목은 "출처 미확인" 으로 표기한다.

---

## purpose

ERC-20 은 명세상 `transfer(amount)` 가 정확히 `amount` 만큼을 수령자에게 이전한다고 가정하지만,
**fee-on-transfer (FoT)** 토큰은 이전 시 수수료를 떼고, **rebasing** 토큰은 `balanceOf` 가
시간/리베이스에 따라 보유 잔액 자체를 변동시킨다. 이 두 비표준 거동은 "내가 서명한 `amount` =
실제 이동/승인 효과" 라는 정적 디코딩의 핵심 가정을 깨뜨린다 — 예: FoT 토큰에 대한 swap 의
min-out 계산이 빗나가거나, rebasing 토큰에 대한 무제한 approve 가 시간이 지나며 의도보다 큰
실효 권한이 된다. `token.metadata` 는 **decode 만으로는 알 수 없는 토큰의 거동 분류**
(`feeBps`, `isRebasing`, `isVerified`) 를 한 번의 enrichment 콜로 산출해, 해당 토큰에 대한
정책이 "이 토큰은 fee-on-transfer 다 / rebasing 이다" 를 근거로 **warn** 을 띄울 수 있게 한다.
정적 분석(no-simulation) 모델을 유지하므로 트랜잭션 트레이스가 아니라 **사실 조회 + 휴리스틱
분류**다.

---

## interface

> 권위 출처: `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §3c (`token.metadata`).
> 아래는 그 항목의 재기술이며, 셀렉터/결과 shape 를 임의로 바꾸지 않는다.

### params (each `$.`-selector + type)

| param | selector | type | 비고 |
|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long | EVM chain id (예: 1, 8453). `eip155:<id>` 로 합성 |
| `asset` | `$.action.token` | AssetRef (String — 토큰 주소) | 분류 대상 ERC-20 컨트랙트 주소 |

- 두 param 모두 manifest 에서 `optional: true` 로 선언 (카탈로그 enrichment 컨벤션).
  셀렉터가 resolve 안 되면 콜 자체가 skip → field 부재 → 정책 inert (dormancy).

### result shape (record fields + types)

```json
{
  "feeBps":     "Long",      // round-trip transfer 시 손실 추정 (basis points, 0 = FoT 아님)
  "isRebasing": "Bool",      // balanceOf 가 리베이스로 변동하는 토큰인가
  "isVerified": "Bool",      // 신뢰 토큰 리스트(레지스트리)에 등재된 토큰인가
  "standard":   "String?"    // optional: "ERC20" | "ERC20-FoT" | "ERC20-Rebasing" 등 라벨
}
```

### projection (record → scalar leaf — MANDATORY)

v2 `materialize_v2` 는 **record ProjectionType 을 허용하지 않는다** (스칼라 전용:
`String | Long | Bool | Decimal | Set<String>`). 따라서 record 를 반환하더라도 manifest 의
`outputs[].from` 가 반드시 **leaf 스칼라**로 투영해야 한다:

| `outputs[].from` | `outputs[].type` (capitalized) | `custom_context` (lowercase Cedar) | context.custom 필드 |
|---|---|---|---|
| `$.result.feeBps` | `Long` | `"Long"` | `feeBps` |
| `$.result.isRebasing` | `Bool` | `"Bool"` | `isRebasing` |
| `$.result.isVerified` | `Bool` | `"Bool"` | `isVerified` |

- 한 정책이 record 전체를 읽을 수 없으므로, 필요한 leaf 마다 별도 `outputs[]` 엔트리를 만든다.
- `outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 강제).
- `standard` 는 진단/리즌-문자열용 optional 필드 — 정책 가드로 쓰려면 `$.result.standard → String`
  추가 투영 필요 (현재 카탈로그 두 정책은 `feeBps` / `isRebasing` 만 가드).

---

## data source(s)

세 갈래를 조합한다. 우선순위: **(1) 신뢰 토큰 리스트 → (2) 알려진 FoT/rebasing 목록 → (3) 온체인 probe**.

| 소스 | 산출 필드 | 분류 | 비고 |
|---|---|---|---|
| **토큰 레지스트리** (TokenMeta) | `isVerified`, `standard` 힌트 | **EXISTING-FETCHER-REUSABLE** — `RegistryFetcher` (`crates/policy-server/sync/src/sources/fetchers/registry.rs`), `DataSource::RegistryApi` + `RegistryResource::TokenMeta { chain, address }` (URL `…/v1/token/<chain>/<addr>`), 24h TTL in-memory cache 내장 | 등재 = `isVerified:true`. 레지스트리가 FoT/rebasing 플래그를 직접 서빙하면 (2)/(3) 생략 가능 |
| **알려진 FoT/rebasing 토큰 목록** (shipped denylist/labellist) | `feeBps`, `isRebasing` (정확) | **NET-NEW plumbing** (정적 데이터 파일 또는 레지스트리 확장 필드) | 가장 신뢰도 높음. 예: stETH/AMPL = rebasing; 알려진 reflection 토큰 = `feeBps>0` |
| **온체인 probe** (eth_call) | `feeBps` (휴리스틱), `isRebasing` (간접 신호) | **EXISTING-FETCHER-REUSABLE** — `OnchainViewFetcher` (`crates/policy-server/sync/src/sources/fetchers/onchain.rs`): `fetch_one`(단건 `eth_call`) / `fetch_batch`(`Multicall::aggregate3`, `0x82ad56cb`, `allow_failure:true`). `OnchainCall::from_source` 가 selector+args 인코딩 | RPC 라우터(`RpcRouter`)·multicall3 주소 설정 필요. probe 자체 정의는 NET-NEW |

> **재사용 요약**: I/O 백본(HTTP 캐시 fetch, `eth_call`, multicall3 배치, ABI 디코딩)은 위 두
> fetcher 로 **이미 존재**한다. NET-NEW 는 (a) `token.metadata` 메서드를 `method`→handler 로 묶는
> `/v1/rpc` 디스패처 엔트리, (b) FoT/rebasing **분류 로직**(목록 + probe 휴리스틱), (c) 레지스트리가
> 아직 안 서빙하면 FoT/rebasing 라벨 데이터 소스다. §5(POLICY_RPC_METHODS.md): in-repo `/v1/rpc`
> 디스패처는 아직 없음 — 위 fetcher 들은 decode-time `live_inputs` 레이어로 **재사용 가능한 빌딩블록**.

---

## derivation algorithm

입력: `chain = eip155:<chain_id>`, `addr = asset`.

1. **레지스트리 조회** — `RegistryFetcher::fetch(RegistryApi{ TokenMeta{chain, addr} })`.
   - 성공 + 등재 → `isVerified = true`. 응답에 FoT/rebasing 플래그가 있으면 `feeBps`/`isRebasing`
     으로 채택하고 step 5 로 점프 (가장 신뢰도 높은 경로).
   - 미등재/404 → `isVerified = false`, 계속.
2. **알려진 목록 매칭** — shipped FoT/rebasing 목록에서 `(chain, addr)` 룩업.
   - rebasing 목록 hit → `isRebasing = true`, `standard = "ERC20-Rebasing"`.
   - FoT 목록 hit → `feeBps = <목록값>`, `standard = "ERC20-FoT"`. (목록은 컨트랙트별 고정 수수료를
     아는 경우만; 모르면 probe 로 추정.)
3. **온체인 FoT probe (휴리스틱)** — 목록에 없고 `feeBps` 미정일 때.
   - `eth_call` (상태 변경 없음, no broadcast) 로 토큰의 표준 view 들을 멀티콜:
     `balanceOf(probe)`, `totalSupply()`, 그리고 가능하면 컨트랙트가 노출하는 수수료 게터
     (`_taxFee()` / `buyTax()` / `sellTax()` / `transferFeeRate()` 등 — 토큰마다 이름 상이).
   - 수수료 게터가 있으면 그 값을 bps 로 정규화해 `feeBps` 산출.
   - 게터가 없으면 `feeBps` 를 **확정할 수 없다** → 추정하지 말고 **필드 미방출** (§failure 참조).
     (정적 `eth_call` 만으로 round-trip 손실을 정확히 재현하려면 실제 transfer 상태변경이 필요한데,
     no-simulation 모델상 그건 범위 밖이다. 따라서 probe 는 "수수료 게터가 노출된 토큰" 만 커버.)
4. **온체인 rebasing 신호 (간접)** — `isRebasing` 미정일 때.
   - rebasing 토큰은 보통 외부 호출로 `balanceOf` 가 변하는 함수(`rebase()` / `setSupply()` 류)나
     share/scaled-balance 구조를 가진다. **확정 시그널이 아니므로** 온체인만으로 `isRebasing=true`
     를 단정하지 않는다 — 알려진 목록(step 2) 으로만 `true` 를 단정하고, 그 외엔 미방출.
5. **조립 + 투영** — 확정된 필드만 result record 에 담는다.
   - `feeBps` (Long), `isRebasing` (Bool), `isVerified` (Bool), `standard?` (String).
   - 각각 §interface 의 leaf 스칼라로 투영되어 `context.custom.*` 로 fold 된다.

### heuristic limits (정직한 한계)

- **FoT 정확도**: 표준 ERC-20 에는 수수료 게터가 없으므로, probe 는 게터를 노출한 토큰만 정량화한다.
  게터 없는 FoT 토큰은 **미탐(false negative)** — `feeBps` 미방출 → 정책 inert (과탐보다 안전).
- **`feeBps` 는 추정치**: 게터 값과 실제 transfer 손실이 buy/sell/transfer 별로 다를 수 있다
  (asymmetric tax). 단일 `feeBps` 는 보수적 단일값(예: max(buyTax,sellTax,transferFee)) 으로 정의.
- **rebasing 단정 금지**: 온체인 구조만으로 rebasing 을 단정하지 않는다. 목록 기반 화이트박스만 `true`.
- 모든 한계는 verdict reason 문자열에 "heuristic: token.metadata probe" 로 명시할 것 (honest UX).

---

## on-chain calls

- **chain**: `eip155:<id>` — `chain_id` param(`$.root.chain_id`) 에서 합성.
- **contract**: `asset` (`$.action.token`) — 분류 대상 ERC-20 그 자체.
- **view fns** (state-changing 아님, `eth_call` only):
  - `totalSupply()` — selector `0x18160ddd`.
  - `balanceOf(address)` — selector `0x70a08231`.
  - (best-effort) 수수료 게터 `_taxFee()` / `buyTax()` / `sellTax()` / `transferFeeRate()` 등 —
    토큰마다 이름이 달라 **고정 selector 가 아님**; `allow_failure:true` 로 다중 후보를 시도.
- **multicall?**: 예 — 후보 게터들을 한 번에 묶는다. `OnchainViewFetcher::fetch_batch` →
  `Multicall::aggregate3` (`0x82ad56cb`), 각 `Call3 { allow_failure:true }` 로 실패 게터는
  graceful 하게 누락. (`rpc/multicall.rs` 참조; multicall3 주소가 체인별로 설정돼야 함.)
- 레지스트리/목록만으로 분류가 끝나면 **on-chain call 0회** — probe 는 fallback 단계에서만.

> 참고: 레지스트리 경로만 쓰는 변형이면 "none (off-chain/data-API)" 로 축소된다. 온체인 probe 는
> FoT 정량화를 위한 **선택적 보강**이다.

---

## caching / ttl

- **key tuple**: `(chain_id, asset_addr_lowercase)` — 토큰 메타는 주소당 사실상 불변(거의 정적)이라
  공격적으로 캐싱해도 안전.
- **ttl**: 레지스트리 경로는 `RegistryFetcher` 의 24h TTL(`DEFAULT_CACHE_TTL`) 을 그대로 승계.
  온체인 probe 결과도 **장기 캐시(≥1h, 24h 권장)** — 토큰의 FoT/rebasing 성질은 변하지 않으므로.
  단, rebasing 토큰의 *동적 잔액*은 캐시하지 않는다(우린 거동 분류만 캐시, 잔액 수치 X).
- **where cached**: `RegistryFetcher` 의 in-memory `RwLock<HashMap>` (이미 존재). probe 분류 결과는
  같은 메서드 핸들러 레벨의 in-memory 맵에 동일 key 로 둔다 (NET-NEW, 동형 패턴).
- **HARD_TIMEOUT_MS=8000 예산 적합성**: 캐시 hit 은 ~0ms. 첫 cold 콜은 레지스트리 1 HTTP(≤10s
  reqwest timeout 이나 실측 수백 ms) + 최대 1 multicall `eth_call`(단일 RTT). 두 I/O 가 직렬이어도
  통상 1초 미만 — 8s budget 내. cold miss 가 budget 을 넘기면 **타임아웃 = 필드 미방출**(dormancy).

---

## failure & fallback (DORMANCY CONTRACT)

핵심 불변식: **에러/누락 시 어떤 필드도 방출하지 않는다.** 절대 verdict 를 뒤집을 수 있는 기본값을
대입하지 않는다.

- 레지스트리 404 / RPC 에러 / probe 실패 / 타임아웃 / param 셀렉터 미resolve 중 무엇이든 →
  해당 leaf field 를 **emit 하지 않는다**.
- 필드 미방출 ⇒ 호스트 fold 가 `context.custom.<field>` 를 채우지 않음 ⇒ 정책의
  `context.custom has feeBps` / `has isRebasing` 가드가 **false** ⇒ 정책 **INERT** (verdict 미생성).
- 결과적으로 dormant/unreachable 디스패처는 **pass 로 degrade** 하지, 거짓 차단(false fail)을
  만들지 않는다. 이는 카탈로그 enrichment 가 전부 `optional: true` 인 이유다 (§wire `Failure
  direction`): optional 누락은 batch 를 hard-fail 시키지 않고 그 필드만 비운다.
- **금지**: `feeBps = 0` 이나 `isRebasing = false` 를 "확정 못 함" 의 기본값으로 방출하는 것
  (→ FoT/rebasing 토큰을 정상 토큰으로 false-PASS 시킬 수 있음). 확정 못 하면 **무방출**이 유일한
  안전 선택. 확정된 음성(예: 레지스트리가 "검증된 표준 ERC20, 비-FoT" 라고 *명시적으로* 보증)만
  `feeBps=0`/`isRebasing=false` 를 방출할 수 있다.
- 부분 성공 허용: `isVerified` 만 확정되고 `feeBps` 미확정이면, `isVerified` 만 방출하고 `feeBps` 는
  비운다 (필드 독립).

---

## auth / cost / rate-limit

- **API keys (env)**: 레지스트리 endpoint(`DataSource::RegistryApi.endpoint`)는 인증이 필요할 수
  있다(레지스트리 배포 정책에 따름 — 출처 미확인). 온체인 probe 는 RPC provider 키가 필요할 수 있다
  (`RpcRouter` provider 설정; 키가 env 인지 config 인지는 배포 의존 — 출처 미확인). FoT/rebasing
  목록이 정적 shipped 데이터면 키 불필요.
- **per-call cost**: 캐시 hit = 0 외부콜. cold = 레지스트리 1 HTTP + (fallback 시) multicall 묶음
  1 `eth_call`. probe 게터를 multicall 로 묶으므로 게터 후보 N개여도 RPC 라운드트립은 1회.
- **rate-limit**: 토큰 메타가 주소당 불변에 가까워 캐시 hit rate 가 매우 높다 → 같은 토큰 반복
  서명 시 외부콜 0. 24h TTL 캐시가 rate-limit 압력을 흡수. 신규 토큰만 cold path.
- **캐시가 비용을 흡수하는 방식**: `(chain, addr)` 키 캐시는 동일 토큰에 대한 모든 후속 액션을
  in-memory 로 응답 → provider/registry 호출량을 "고유 토큰 수" 로 상한.

---

## activation

이 메서드를 구현하면 아래 **dormant 카탈로그 정책 2개**가 un-dormant 된다 (POLICY_RPC_METHODS.md §4):

| 카탈로그 정책 id | action/domain | 가드 (읽는 필드) | verdict |
|---|---|---|---|
| `fee-on-transfer-token` (I1) | action / token | `context.custom has feeBps` && `feeBps > 0` | **warn** |
| `rebasing-token-approve` (I2) | action / token | `context.custom has isRebasing` && `isRebasing` | **warn** |

구현 = (1) `schema/method-catalog.json` 에 `token.metadata` 등록 (현재 미등록 — aspirational),
(2) `/v1/rpc` 디스패처에 `method == "token.metadata"` 핸들러 추가, (3) 위 데이터 소스/알고리즘 배선.

---

## primary-source references

- **EIP-20 (ERC-20 Token Standard)** — `transfer`/`balanceOf`/`totalSupply` 표준 시맨틱. FoT/rebasing
  이 깨뜨리는 "amount = 정확 이전량" 가정의 출처. https://eips.ethereum.org/EIPS/eip-20
- **In-repo: `crates/policy-server/sync/src/sources/fetchers/registry.rs`** — `RegistryFetcher`,
  `RegistryResource::TokenMeta { chain, address }`, URL `…/v1/token/<chain>/<addr>`, 24h TTL 캐시
  (재사용 빌딩블록).
- **In-repo: `crates/policy-server/sync/src/sources/fetchers/onchain.rs`** — `OnchainViewFetcher`
  (`fetch_one`/`fetch_batch`), `OnchainCall::from_source` (selector+args 인코딩) (재사용 빌딩블록).
- **In-repo: `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs`** — `Multicall::aggregate3`
  (selector `0x82ad56cb`, `Call3{ allow_failure }`) — probe 게터 배치 (재사용 빌딩블록).
- **In-repo: `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §3c** — `token.metadata`
  와이어 인터페이스(params/result/projection) 의 권위 출처.
- **Multicall3 (`aggregate3`)** — `0x82ad56cb` selector 및 `Call3`/`Result` ABI. 코드 주석 검증;
  외부 표준 컨트랙트 명세는 출처 미확인(공식 multicall3 문서 미인용).
- **rebasing/FoT 토큰 라벨 데이터(예: stETH, AMPL, reflection 토큰)** — 구체 토큰별 수수료/리베이스
  파라미터는 **출처 미확인** (각 토큰 공식 docs 로 개별 검증 필요; 본 스펙은 데이터 소스 *형태*만 규정).
