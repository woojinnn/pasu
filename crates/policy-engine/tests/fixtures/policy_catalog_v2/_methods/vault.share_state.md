# method: vault.share_state

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> 이 문서는 `/v1/rpc` 서버 구현자가 읽는 **HOW** 스펙이다. 와이어 인터페이스는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§3c `vault.share_state`)
> 에서 이미 정의됐다. 여기서는 그 값을 **어떻게 계산하는지**를 다룬다.
> 참조 정책: `policy_catalog_v2/action/token/supply-empty-vault-inflation/` (manifest.json + policy.cedar).

## purpose

ERC-4626 (또는 4626-style) vault 에 **첫/초기 depositor** 로 들어갈 때 발생하는 **share-inflation
("donation") 공격** 위험을 한 개의 Bool 플래그(`inflationRisk`) 로 압축해 정책에 공급한다.
vault 의 `totalSupply()` (발행된 share 수) 가 0 이거나 극소수일 때, 공격자가 vault 의 자산 토큰을
직접 transfer(donation)해 `totalAssets()/totalSupply()` 비율(=share price)을 부풀리면, 뒤이어 들어오는
첫 예치자는 반올림(rounding-down) 때문에 0 share 를 받거나 심하게 희석된다. 정적 calldata 만으로는
"이 vault 가 지금 비어 있는가" 를 알 수 없다 — 이는 **체인 상태**(현재 supply/assets)에 달려 있다.
`vault.share_state` 는 그 상태를 한 번 읽어 near-empty 휴리스틱으로 판정한 결과를 돌려준다. 정책
`supply-empty-vault-inflation` (severity=warn) 은 `inflationRisk == true` 일 때만 사용자에게 경고한다.
**시뮬레이션이 아니라 fact-fetch** — ScopeBall 의 no-simulation 모델과 일치한다.

## interface

(와이어 계약 출처: POLICY_RPC_METHODS.md §3c; 참조 manifest 의 `policy_rpc[0]`.)

### params (manifest `params`, 각 `$.`-selector)
| key | selector | type | 의미 |
|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long | EVM chain id (caip2 `eip155:<id>` 의 숫자부). on-chain call 의 chain 결정. |
| `venue` | `$.action.venue` | String (VenueRef) | vault 컨트랙트를 식별하는 venue. **vault 주소를 여기서 resolve** 한다 (아래 data source 참조). |
| `asset` | `$.action.asset` | AssetRef | 예치되는 underlying asset. venue→vault 매핑이 모호할 때 disambiguation 에 쓰고, `totalAssets()` 의 단위(decimals) 해석에 쓴다. |

### result shape (record)
| field | type | 의미 |
|---|---|---|
| `totalSupply` | String | vault 가 발행한 share 총량 (uint256 십진 문자열; overflow 회피 위해 number 아님). |
| `totalAssets` | String | vault 가 보유한 underlying asset 총량 (uint256 십진 문자열). |
| `inflationRisk` | Bool | near-empty 휴리스틱 판정 결과 (아래 derivation). 정책이 읽는 유일한 leaf. |

### projection (record → scalar leaf — **mandatory**)
v2 `materialize_v2` 는 **scalar** projection type 만 `context.custom.*` 로 받는다
(`String | Long | Bool | Decimal | Set<String>`). record ProjectionType 은 존재하지 않으므로
record 를 반드시 leaf 로 사영해야 한다. 참조 manifest 가 사영하는 leaf:

- `$.result.inflationRisk -> Bool`  (manifest `outputs[0]`: field=`inflationRisk`, type=`Bool`,
  from=`$.result.inflationRisk`, required=`false`; `custom_context.fields.inflationRisk = "Bool"`)

추가로(미래 정책/디버깅용, 현 catalog 미사용이나 result 에 포함 권장):
- `$.result.totalAssets -> String` (사영 시 type=`String`, custom_context lowercase=`"String"`)

> `outputs[].field` ⇄ `custom_context.fields` 는 1:1 (ManifestV2::validate 강제).
> `outputs[].type` 는 capitalized(`Bool`), `custom_context` 철자는 lowercase Cedar(`"Bool"`).

## data source(s)

**EXISTING-FETCHER-REUSABLE** — `crates/policy-server/sync/src/sources/fetchers/onchain.rs`
`OnchainViewFetcher`. 이미 필요한 plumbing 을 전부 갖췄다:

- `OnchainCall::from_source(DataSource::OnchainView{chain, contract, function, decoder_id}, args)`
  — `function` 의 4-byte selector + ABI-encoded args 로 calldata 조립.
  `totalSupply()` selector=`0x18160ddd`, `totalAssets()` selector=`0x01e1d114` (args 없음 → calldata=selector only). 둘 다 `from_source` 단위테스트가 totalSupply 케이스를 이미 커버.
- `OnchainViewFetcher::fetch_batch(chain, &[OnchainCall])` — **Multicall3 `aggregate3`** 로 두 call 을
  한 round-trip 에 묶고, per-call `allow_failure=true` (한쪽 실패가 다른 쪽을 죽이지 않음).
  `fetch_one` 은 단일 `eth_call` 경로.
- decode: `decoder_id="u256"` (builtin `DecoderRegistry::with_builtins()` → `decode_u256_as_string`)
  가 returndata 32-byte word 를 **십진 String** 으로 디코드 → 그대로 result 의 `totalSupply`/`totalAssets`.
- RPC failover: `RpcRouter` (router.rs) 가 chain 별 provider fan-out + health-tracking 제공.
  multicall 주소는 `RpcRouter::multicall_addr(chain)` (config `multicall_addr`).

**NET-NEW plumbing** (이 fetcher 가 안 해주는 부분):
1. **venue → vault address resolution.** `params.venue` 를 실제 vault 컨트랙트 주소로 매핑하는
   registry lookup. `fetchers/registry.rs` (RegistryFetcher) 또는 manifest bundle 의 venue→address
   테이블을 재사용. (resolution 실패 시 §failure 의 dormancy 경로로.)
2. **`/v1/rpc` method dispatcher 자체.** 현재 in-repo 에 `/v1/rpc` method registry 가 **없다**
   (POLICY_RPC_METHODS.md §5). `method` 문자열로 dispatch → `params` 파싱 → `OnchainViewFetcher`
   호출 → result record 조립 → `{id, ok, result}` 반환하는 핸들러를 새로 작성해야 한다.
3. **near-empty 휴리스틱** (아래 derivation) — 순수 산술, 외부 의존 없음.

## derivation algorithm

입력: `chain_id`, vault 주소 `V` (venue resolve 결과), underlying `asset`.

1. `chain := eip155:<chain_id>`.
2. vault `V` 에 대해 두 view 를 **하나의 multicall** 로 읽는다 (`fetch_batch`):
   - `call_a = totalSupply()` → `decoder_id="u256"` → `tsupply: String`
   - `call_b = totalAssets()` → `decoder_id="u256"` → `tassets: String`
3. 둘 다 성공 시:
   - `s := parse_u256(tsupply)`, `a := parse_u256(tassets)`.
   - **near-empty 판정 (heuristic):**
     - `inflationRisk := (s == 0) || (s < SHARE_FLOOR)`
       - 1차 신호는 `totalSupply == 0` (진짜 first-depositor — 가장 명확한 위험).
       - 2차 신호 `s < SHARE_FLOOR` 는 "거의 비었지만 0 은 아닌" vault 도 포착.
         **권장 기본값 `SHARE_FLOOR = 1e6` (raw share units, ≈ 1e-12 share @ 18dec).**
         이 값은 OZ ERC-4626 의 `_decimalsOffset` virtual-share 완화책(1e6 virtual offset)과
         같은 자릿수 — virtual-share 보호가 없는 vault 에서 의미 있는 floor.
   - result = `{ totalSupply: tsupply, totalAssets: tassets, inflationRisk }`.
4. 한쪽 call 만 성공(예: `totalAssets()` 미구현 — 4626 가 아닌 컨트랙트):
   - `totalSupply` 만으로 `inflationRisk := (s == 0) || (s < SHARE_FLOOR)` 판정,
     `totalAssets` 은 빈 문자열/생략. (totalSupply 가 핵심 신호이므로 충분.)
5. **양쪽 다 실패 / vault resolve 실패** → §failure (필드 미발행).

**정직한 한계 (heuristic limits):**
- `SHARE_FLOOR` 는 vault decimals/규모에 무관한 **고정 raw floor** 라, 매우 작은 단위의
  정상 vault 를 false-positive 로, 매우 큰 단위의 위험 vault 를 false-negative 로 오판할 수 있다.
  정책 severity 가 **warn** (block 아님) 이라 이 부정확성이 사용자를 hard-block 하지 않는 점이 안전판.
- virtual-share / `_decimalsOffset` 보호가 **이미 적용된** vault (OZ 4626 ≥4.9) 는 사실상 위험이
  완화돼 있으나, 이 휴리스틱은 그 보호 여부를 보지 않는다 → 보호된 vault 도 near-empty 면 warn.
  과경고이지 미경고는 아니므로 보수적(safe) 방향. verdict reason 에 "heuristic" 임을 명시할 것.
- `share price = totalAssets/totalSupply` 비율 자체를 임계로 쓰지 **않는다** (donation 직후 비율은
  이미 부풀려져 있어 신호가 늦음). supply 의 absolute floor 가 더 이른 신호.

## on-chain calls

- chain: `eip155:<chain_id>` (param `$.root.chain_id` 에서).
- contract: vault 주소 `V` (param `$.action.venue` resolve 결과).
- view fns:
  - `totalSupply()` → `uint256` (selector `0x18160ddd`)
  - `totalAssets()` → `uint256` (selector `0x01e1d114`, EIP-4626)
- **multicall: YES** — Multicall3 `aggregate3`, per-call `allow_failure=true`, `BlockTag::Latest`
  (`OnchainViewFetcher::fetch_batch`). 두 view 를 1 RPC round-trip 으로.

## caching / ttl

- **key tuple:** `(chain_id, vault_address)`. (asset/venue 가 같은 vault 로 resolve 되면 같은 키 —
  vault 주소가 cache identity. `block=latest` 이므로 block 은 키에 넣지 않되 ttl 로 신선도 관리.)
- **ttl:** 짧게 — **권장 15s** (near-empty 상태는 첫 예치로 빠르게 변할 수 있다; stale 한 "비었다"
  판정이 위험을 놓치지 않도록 보수적으로 짧게). 단일 vault 의 동일 action 내 중복 plan 은 캐시 히트.
- **where cached:** `/v1/rpc` 서버 프로세스 in-memory (per-method LRU/TTL map). 이는 host-side 캐시이며
  WASM engine state 와 무관.
- **HARD_TIMEOUT_MS=8000 budget:** multicall 1 round-trip (typically <500ms public RPC) + venue
  resolve(캐시 시 ~0) ≪ 8000ms. cold-miss 라도 단일 multicall 이라 budget 여유. provider 무응답 시
  내부 timeout 을 budget 보다 작게(예 3s) 잡아 §failure 로 빠지게 할 것 — 8s 오케스트레이터 hard
  timeout 에 걸리면 안 됨.

## failure & fallback (DORMANCY CONTRACT)

오류·미싱·resolve 실패 시 **해당 필드를 발행하지 않는다.** 연쇄:

```
on error / vault unresolved / both calls fail
  → emit NO `inflationRisk` field in result (or ok:false for the call)
  → host fold: required projection 부재 ⇒ map[call_id] 에 inflationRisk 없음
  → context.custom 에 inflationRisk 없음
  → policy guard `context.custom has inflationRisk` == false
  → policy INERT (절대 false verdict 아님)
```

- 참조 manifest 는 `optional: true` 이고 output 은 `required: false` → **optional enrichment**.
  POLICY_RPC_METHODS.md §1 "Failure direction": optional call 누락은 그냥 필드 부재 → 정책 inert →
  **pass 로 degrade**, hard batch fail 아님. 도르먼트/도달불가 dispatcher 가 spurious block 을
  만들지 않도록 catalog 가 의도적으로 optional 로 작성됨.
- **절대 금지:** verdict 를 뒤집을 수 있는 **default 대체 금지**. 특히
  `inflationRisk = false` 를 fallback 으로 채워 넣지 말 것 — 그러면 "비었지만 못 읽음" 케이스가
  조용히 "안전"으로 둔갑한다. **읽지 못하면 필드를 빼라** (정책이 inert 가 되어 경고를 안 할 뿐,
  거짓 안전 신호를 주지 않음).
- 반대로 `inflationRisk = true` default 도 금지 (정상 vault 를 매번 warn → 사용자 피로 + 거짓 신호).
- 즉 **세 번째 상태(unknown)=필드 부재** 를 명시적으로 유지하라. Bool 두 값 중 하나로 강제 붕괴 금지.

## auth / cost / rate-limit

- **API keys (env):** 없음(기본). public RPC provider (`PublicRpcProvider`, kind=`"public"`) 로
  동작 — config TOML `[chains."eip155:<id>".providers]`. Alchemy/Infura 등 인증 provider 는
  router 가 `instantiate_provider` 의 주석된 슬롯으로 확장 가능(현재 미구현) → 그 경우만 키 env 필요.
- **per-call cost:** multicall 1회 = `eth_call` 1 RPC. 무료/저비용. 외부 가격 API·인덱서 불필요
  (순수 on-chain view + 로컬 산술).
- **rate-limit:** public RPC 의 IP rate-limit 만 변수. 위 caching(15s, key=vault) 이 동일 vault 반복
  조회를 흡수해 burst 를 평탄화. 여러 vault 동시 plan 시 chain 별 router failover 가 단일 provider
  과부하를 분산.

## activation

이 method 를 구현(+ method-catalog.json 등록)하면 다음 catalog 정책이 un-dormant 된다:

- **`supply-empty-vault-inflation`** (`action/token/`, severity=warn) — `context.custom.inflationRisk == true`
  일 때 forbid(`Lending::Action::"Supply"`). 활성화 전까지는 `context.custom has inflationRisk` 가
  false 라 inert.

(POLICY_RPC_METHODS.md §4 activation map: `vault.share_state` = **new**, 1 policy.)

## primary-source references

- EIP-4626: Tokenized Vault Standard — `totalSupply()`/`totalAssets()`/`convertToShares` 정의 및
  보안 고려(Security Considerations) 의 inflation/donation 공격 서술.
  https://eips.ethereum.org/EIPS/eip-4626  (§Methods, §Security Considerations)
- ERC-20 `totalSupply()` (selector `0x18160ddd`) — EIP-20.
  https://eips.ethereum.org/EIPS/eip-20
- OpenZeppelin ERC4626 `_decimalsOffset` / virtual-shares 완화책 (SHARE_FLOOR 자릿수 근거):
  OpenZeppelin Contracts docs — ERC4626. https://docs.openzeppelin.com/contracts/5.x/erc4626
- Multicall3 `aggregate3` (allow_failure 시맨틱) — `https://github.com/mds1/multicall` (Multicall3 spec).
- in-repo plumbing (1차, 코드): `crates/policy-server/sync/src/sources/fetchers/onchain.rs`
  (`OnchainViewFetcher::fetch_batch`/`from_source`), `.../rpc/router.rs` (`RpcRouter::eth_call`),
  `.../rpc/multicall.rs` (`Multicall::aggregate3`), `.../fetchers/decoder.rs` (`u256` builtin).
- 와이어 계약(1차, 문서): `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §3c, §1, §5.

> SHARE_FLOOR=1e6 의 "정확한 false-positive/negative 경계" 는 vault 별 decimals·규모 의존이라
> 단일 보편값으로 증명 불가 — **출처 미확인** (휴리스틱 튜닝 파라미터로 명시; warn-severity 가 안전판).
