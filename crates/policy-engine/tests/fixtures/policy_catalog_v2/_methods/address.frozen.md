# method: address.frozen

status: aspirational (NOT yet in `schema/method-catalog.json`; NOT yet referenced by a shipped
catalog `manifest.json` — POLICY_RPC_METHODS.md §3/§4 의 활성 메서드 표/activation map 에 없음)

> 미래의 `/v1/rpc` 서버 구현자가 읽는 **구현 스펙**이다. wire interface 만이 아니라 *어떻게* 만드는지를
> 기술한다. wire 계약/projection 제약/dormancy 의 1차 출처는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §6) 이며,
> 재사용 가능한 on-chain plumbing 의 1차 출처는
> `crates/policy-server/sync/src/sources/fetchers/{onchain,decoder,rpc}` 코드다.
> `.md` 는 repo 전역 gitignore 라 이 파일은 standalone artifact 로 이동한다.
> **aspirational 표식의 의미:** 이 메서드를 호출하는 catalog 정책은 아직 published 카탈로그에 없다.
> 따라서 아래 "activation" 은 *이 메서드를 도입하면서 함께 작성할* compliance/issuer-freeze 정책군의
> 설계이지, 오늘 dormant 상태로 대기 중인 기존 정책의 해제가 아니다. 검증 못 한 것은 "출처 미확인" 으로 명시했다.

---

## purpose

한 주소가 **stablecoin 발행사(issuer)의 on-chain 동결(freeze/blacklist) 목록에 올라 있는지**를
알려준다. 규제형 stablecoin (USDC, USDT 등) 은 발행사가 컨트랙트에 박아둔 blocklist getter 로 임의
주소를 동결할 수 있다 — Circle USDC `FiatTokenV2` 의 `isBlacklisted(address)` [1], Tether USDT 의
`isBlackListed(address)` [2]. 동결된 주소가 관련되면 그 자산의 `transfer`/`transferFrom`/`approve`
경로가 revert 하거나, 더 나쁘게는 **사용자 자신이 동결 대상**이라 보유분이 묶여 있는데도 그걸 모른 채
새 권한을 위임/이체하려 들 수 있다.

정적 calldata 만으로는 "지금 누구에게 얼마를 보낸다/허용한다" 만 보일 뿐, **체인 현재 상태의 동결
여부**는 알 수 없다. 따라서 이 fact 는 토큰 컨트랙트의 blocklist view 를 1 회 읽어 enrichment 로
주입해야 한다. ScopeBall 의 no-simulation 모델과 일관되게 이것은 **순수 fact-fetch (단일 `eth_call` +
bool decode)** 이지 트랜잭션 시뮬레이션이 아니다.

---

## interface

1차 출처: 이 spec (메서드가 아직 `method-catalog.json` 에 없음 — status 참조) + wire/projection 제약은
`POLICY_RPC_METHODS.md` §1, §2.

### params (manifest 가 넘기는 selector — 설계값)

| param | selector (설계) | type | 의미 |
|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long (eip155 numeric id) | 어느 체인의 토큰 컨트랙트를 읽을지. on-chain read 대상 체인. |
| `asset` | `$.action.token` / `$.action.asset` | AssetRef/String (stablecoin ERC-20 address) | blocklist getter 를 호출할 **토큰 컨트랙트** (= `eth_call` 의 `to`). USDC/USDT 등 issuer-freeze 가능 토큰. |
| `address` | `$.action.spender` / `$.action.recipient` / `$.root.from` | String (address) | 동결 여부를 조회할 주소. 정책별로 spender(approve 대상) · recipient(이체 대상) · self(`from`) 중 하나를 가리킨다 (아래 activation 의 3 분기). |

> `chain_id` 는 셀렉터 상 Long 이지만 라우터는 `ChainId` = `"eip155:<id>"` 문자열로 key 한다
> (`RpcRouter`). 핸들러는 `chain_id` 를 받아 `eip155:<id>` 로 정규화해서 라우터에 넘긴다.
> `asset` 가 ActionView 상 AssetRef 면 핸들러는 ERC-20 `address` 만 추출한다 (ActionView spelling 의
> 권위는 `/v1/rpc` planner — POLICY_RPC_METHODS.md §3a "Note on selectors", §6 "Selector roots").
> 동일 메서드를 caller 가 어느 주소(spender/recipient/self)에 대해 부르는지는 manifest 의 `address`
> selector 가 결정한다 — 핸들러는 받은 주소 1 개만 조회하고 의미는 정책에 위임한다.

### result shape (핸들러가 내는 record)

```json
{
  "frozen": true,                 // Bool   — 토큰 컨트랙트가 이 주소를 blocklist 에 올렸는지
  "asset": "0xA0b8...eB48",       // String — optional. 실제 조회한 토큰 (debug)
  "getter": "isBlacklisted",      // String — optional. 실제 호출한 getter (USDC=isBlacklisted, USDT=isBlackListed)
  "checkedTs": 1717400000         // Long   — optional. 조회 시각 (staleness 표면화용)
}
```

- `frozen` : **필수.** 정책이 읽는 유일한 leaf. `bool` decoder 가 returndata word 의 LSB 에서
  뽑은 값 (`decode_bool`, decoder.rs:95).
- `asset`/`getter`/`checkedTs` : 전부 부가정보 — v2 엔진은 record 를 통째로 못 받으므로(아래 projection)
  정책 verdict 에 영향이 없다. 구현 1차 컷은 `frozen` 만 채워도 된다.

### projection: `$.result.frozen → Bool` (record → scalar leaf — MANDATORY)

v2 의 `materialize_v2` 는 record ProjectionType 을 받지 않는다 (`String | Long | Bool | Decimal |
Set<String>` 만 — POLICY_RPC_METHODS.md §2). 그러므로 record 는 manifest `outputs[].from` 으로
**반드시 leaf scalar** 까지 내려야 한다.

- projection: `$.result.frozen → Bool` (예 field `addressFrozen`, lowercase Cedar `"Bool"`).
- `outputs[].field` ⇄ `custom_context.fields` 는 1:1 (`ManifestV2::validate` 강제 — §2).
- 즉 핸들러가 어떤 record 를 돌려주든 정책이 실제로 보는 것은 `context.custom.addressFrozen : bool`
  하나다.

---

## data source(s)

**EXISTING-FETCHER-REUSABLE.** 새 네트워크 plumbing 0. 단일 `eth_call` + 기존 `bool` decoder 만으로
완결된다 — 다음 building block 을 그대로 재사용:

- `crates/policy-server/sync/src/sources/fetchers/onchain.rs` —
  `OnchainViewFetcher::fetch_one` / `fetch_batch`. `DataSource::OnchainView { chain, contract,
  function, decoder_id }` 로부터 `OnchainCall::from_source` 가 selector(`function_selector(function)`)
  + abi-encoded args 를 붙여 `OnchainCall` 을 만들고, `fetch_one` 이 `RpcRouter::eth_call(chain, …)`
  한 뒤 `decoder_id` 로 returndata 를 decode 한다 (onchain.rs:112-116).
- `crates/policy-server/sync/src/sources/fetchers/decoder.rs` —
  - `bool` decoder 가 **이미 등록**됨 (`DecoderRegistry::with_builtins`, decoder.rs:64 → `decode_bool`
    decoder.rs:95-103). returndata ≥32 byte 중 word LSB(`data[31] != 0`)로 bool. **신규 decoder
    불필요.**
  - args 인코딩: `encode_address(address)` (20-byte addr → 32-byte left-pad, decoder.rs:27-32).
  - selector: `function_selector("isBlacklisted(address)")` / `function_selector("isBlackListed(address)")`
    (decoder.rs:11-17, keccak256 앞 4 byte). 두 getter 는 **철자가 다르다** — USDC=`isBlacklisted`,
    USDT=`isBlackListed` (대문자 L). 셀렉터 정확값은 아래 "on-chain calls" 참조 (in-repo 상수 없음 →
    **출처 미확인 / 구현 시 keccak 검증**).
- `crates/policy-server/sync/src/sources/fetchers/rpc/` — `RpcRouter` (체인별 provider failover) +
  `rpc/multicall.rs` `Multicall::aggregate3` (한 액션에 frozen 조회 ≥2 개일 때 `fetch_batch` 가 자동
  사용; 단건이면 `fetch_one`).

> 이 fetcher 들은 **decode-time** `live_inputs` enrichment 용으로 만들어졌다 (POLICY_RPC_METHODS.md
> §5). `/v1/rpc` 메서드 dispatcher 자체는 아직 in-repo 에 없다 — 구현 = `method` 로 keying 하는
> dispatcher 를 만들고, `address.frozen` 핸들러가 위 fetcher 들을 호출하도록 배선하는 것.

### NET-NEW (이 메서드 고유)

- `/v1/rpc` method dispatcher 자체 (오늘 repo 에 **없음** — POLICY_RPC_METHODS.md §5).
- **getter 분기 테이블**: `(chain_id, asset_address) → ("isBlacklisted" | "isBlackListed")`. 토큰마다
  blocklist getter 의 철자/존재가 다르므로 — issuer-freeze 가능 토큰(주소)별로 어떤 getter 를 호출할지
  매핑해야 한다. 매핑에 없는 토큰은 blocklist 개념이 없거나 미상 → **조회 생략**(아래 dormancy contract).
  - 초기 후보(검증 권장): Ethereum mainnet USDC `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`
    → `isBlacklisted`; USDT `0xdAC17F958D2ee523a2206206994597C13D831ec7` → `isBlackListed`. 정확
    주소/getter 존재는 **구현 시 컨트랙트 ABI 로 검증** (아래 references; 본 spec 의 주소 문자열은
    참조용, 1차 ABI 로 재확인).

---

## derivation algorithm

입력: `chain_id` (Long), `asset` (AssetRef/address), `address` (address). 모두 optional selector
(`policy_rpc[].optional: true`).

1. **정규화.** `chain` ← `ChainId("eip155:" + chain_id)`. `asset`/`address` ← 20-byte `Address`
   파싱 (checksum 무관, lower-case 비교). 셀렉터 누락이면 — dormancy contract — call 자체를 skip.
2. **getter 결정.** `(chain, asset)` 를 위 NET-NEW 분기 테이블에서 찾아 `function` 문자열 확정
   (`"isBlacklisted(address)"` 또는 `"isBlackListed(address)"`). 매핑에 없으면 **조회하지 않고
   결과를 비운다** (frozen 필드 미emit → 정책 INERT). **추측 default(예 frozen=false) 금지.**
3. **calldata 구성.** `args = encode_address(address)` (32 bytes). `decoder_id = "bool"`.
   `OnchainCall::from_source(DataSource::OnchainView { chain, contract: asset, function, decoder_id },
   args)` — selector 는 `function_selector(function)` 가 붙인다 (onchain.rs:29-38).
4. **on-chain read.** `OnchainViewFetcher::fetch_one(&call)` → `eth_call(asset, calldata)` at
   `BlockTag::Latest`. (한 액션에 ≥2 frozen 조회면 `fetch_batch` → multicall3 `aggregate3`,
   `allow_failure=true`.)
5. **decode.** `bool` → `decode_bool` → `frozen: Bool` (returndata word LSB).
6. **record 조립** 후 반환: `{ frozen, asset, getter, checkedTs }`. host fold 는 record 를
   `map[call_id]` 로 넣고, manifest projection `$.result.frozen → Bool` 이 `addressFrozen` leaf 를 뽑는다.

### heuristic / honest limits (정직한 한계)

- **issuer-freeze 범위만.** 이 메서드는 *발행사가 그 토큰 컨트랙트에 박은* blocklist 만 본다. OFAC
  제재 주소 전반, 분석사(Chainalysis/TRM) 위험 점수, 다른 프로토콜의 자체 차단 목록은 **다루지 않는다** —
  그건 `address.reputation` (POLICY_RPC_METHODS.md §3c, 별도 외부 feed) 의 영역이다. frozen=false 는
  "이 토큰 발행사가 동결 안 함" 일 뿐 "안전/제재 없음" 이 아니다.
- **getter 커버리지 = 메서드 커버리지.** 분기 테이블에 없는 토큰은 항상 결과가 비어 정책이 inert 다.
  매핑이 곧 커버리지 — 과장 금지.
- **시점.** `BlockTag::Latest` 의 동결 상태다. 서명~실행 사이에 발행사가 동결/해제하면 결과가 stale 할
   수 있다 (staleness 차단이 필요하면 `checkedTs` 로 표면화하되, 여기서 임의 default 로 막지 않는다).
- **프록시/업그레이드.** USDC/USDT 는 upgradeable proxy 다 — 미래 구현이 getter 철자/존재를 바꿀 수
  있다. 분기 테이블은 ABI 기준으로 유지·검증해야 한다 (출처 미확인 가정 금지).

---

## on-chain calls

- **chain**: `eip155:<chain_id>` — `chain_id` param (`$.root.chain_id`) 에서 도출.
- **contract**: `asset` (stablecoin ERC-20 주소).
- **view fn (토큰별 분기)**:
  - USDC (Circle `FiatTokenV2`): `isBlacklisted(address) returns (bool)` [1]. selector =
    `keccak256("isBlacklisted(address)")[..4]` — 정확 4-byte 값은 in-repo 상수 없음, **구현 시
    `function_selector` 로 산출/검증** (decoder.rs:11-17). **출처 미확인** (셀렉터 hex 문자열).
  - USDT (Tether): `isBlackListed(address) returns (bool)` [2] (대문자 `L`). selector =
    `keccak256("isBlackListed(address)")[..4]` — 동일하게 구현 시 산출/검증. **출처 미확인** (hex).
  - 두 함수 모두 단일 `address` arg, 단일 `bool` return → `bool` decoder 그대로.
- **multicall?**: 단건이면 직접 `eth_call`. 같은 체인에서 한 액션에 frozen 조회 ≥2 개(예 self + recipient)
  면 `fetch_batch` 가 multicall3 (`aggregate3`, `allow_failure=true`) 로 1 RPC 에 묶는다. 체인 mismatch
  시 batch 거부 (onchain.rs:127-134).
- block tag: `Latest` (현재 상태 — pending mempool 아님, 정적 모델 일관).

---

## caching / ttl

- **cache key tuple**: `(chain_id, asset, address, getter, blockTag=Latest)`.
- **ttl**: 짧게 — 동결 상태는 발행사가 언제든 바꾼다. 권장 **≤ 30 s** (단일 pre-sign 흐름 내 중복
  dispatch 흡수가 목적이지 장기 캐시 아님). 더 길면 stale 동결로 정책이 잘못 inert/active 될 수 있다.
  **출처 미확인** (본 repo 코드에 명시 상수 없는 권장값).
- **where**: `/v1/rpc` 서버 프로세스 in-memory (per-method LRU/TTL map). 익스텐션 host 의
  `dispatchCallsV2` 는 per-action fresh results map 을 유지하므로 (CLAUDE.md "fresh per-action
  results map") cross-action 누수는 서버 측 책임.
- **budget**: orchestrator `HARD_TIMEOUT_MS = 8000`. 단순 `eth_call` 1 회 read 는 보통 수백 ms;
  multicall batch + provider failover 로 round-trip 1 회면 예산 내. cache hit 은 0 RPC. (단,
  공개 provider client 의 자체 timeout 이 8 s 보다 길면 핸들러가 per-call deadline 을 더 작게 잡아야
  함 — `approval.allowance` spec 의 동일 주의 참조.)

---

## failure & fallback (DORMANCY CONTRACT)

이 메서드는 **optional** (`policy_rpc[].optional: true` — catalog enrichment 표준, POLICY_RPC_METHODS.md
§1 "Failure direction", §6 "optional: true"). 실패/누락 시 **절대 verdict 를 뒤집을 수 있는 default 를
넣지 않는다.**

- RPC 실패 / decode 실패 / `asset`·`address`·`chain_id` 셀렉터 미해결 / getter 분기 테이블에 토큰
  없음 / 체인 미지원 / 컨트랙트에 getter 부재(revert) → **`frozen` 필드를 emit 하지 않는다**
  (record 에서 누락 또는 result 통째 비움 → `ok:false`).
- host fold: 필드 부재 ⇒ `context.custom` 에 `addressFrozen` 가 안 들어감 ⇒ Cedar 의
  `context.custom has addressFrozen` 가드가 **false** ⇒ 그 정책은 **INERT** (절대 거짓 verdict 없음).
  dormant 정책은 false verdict 를 만들지 않는다.
- **금지**: 못 읽었을 때 `frozen = false` (안전해 보이는 default) 를 넣는 것 — 그건 실제로 동결된
  주소를 "정상" 으로 가려 정책을 silently false-pass 시킨다. 반대로 `frozen = true` default 도
  금지(false fail). 못 읽으면 **field 부재 = 정책 침묵** 이 유일한 올바른 동작.
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. 이 enrichment 는
  cap/compliance 신호일 뿐 venue 차단 로직이 아니므로 deny-closed(HyperLiquid) 방향과 무관하다 —
  `optional:true` 덕에 missing input 은 batch hard-fail 이 아니라 **해당 정책만 inert** 로 degrade 한다.

---

## auth / cost / rate-limit

- **API keys (env)**: 기본 경로(`kind="public"` provider, 예 publicnode)는 **키 불필요**. provider 설정은
  `RpcConfig` (TOML, `[chains."eip155:<id>".providers]`) 로 주입. Alchemy/Infura 등 키 기반 provider 는
  `instantiate_provider` stub 을 채워야 하며 그쪽이 env (`ALCHEMY_API_KEY` 등) 의존 — **NET-NEW**,
  기본 동작엔 불필요. (외부 분석사 freeze API 는 쓰지 않는다 — 순수 on-chain read.)
- **per-call cost**: `frozen` 1 조회 = `eth_call` 1 회 (또는 multicall 로 N 조회 = 1 회). 무상태·저비용.
  cache hit 은 0 네트워크.
- **rate-limit**: 공개 RPC 는 IP 기반 rate-limit 이 있을 수 있음. `RpcRouter` 의 provider failover 가
  한 provider throttle/down 시 다음 priority 로 넘긴다. ≤30 s TTL 캐시가 같은 pre-sign 흐름의 중복
  조회를 흡수해 호출량을 줄인다.

---

## activation

> **status 재확인:** 이 메서드와 그 정책들은 아직 published 카탈로그/`POLICY_RPC_METHODS.md` §4
> activation map 에 없다. 아래는 **이 메서드를 도입하면서 함께 작성할** compliance / issuer-freeze
> 정책군의 설계다 (4 정책). 각 정책은 동일 `address.frozen` 메서드를 **다른 `address` selector** 로
> 호출한다 — 핸들러는 1 주소만 조회하고 의미는 정책이 결정한다. 모두 `$.result.frozen → Bool`
> (`context.custom.addressFrozen`) 를 project 하고 `addressFrozen == true` 일 때 fire 한다.

- **`usdc-blocklist-spender`** — action / token (approve·permit). `address` = `$.action.spender`.
  approve 대상(spender)이 USDC blocklist 에 있으면 그 권한은 사실상 무용/위험 → **warn** (또는
  정책 작성자가 deny 로 격상). getter = `isBlacklisted`.
- **`usdt-blocklist-spender`** — 위와 동형, asset=USDT, getter = `isBlackListed`. (USDC/USDT 를 한
  정책에서 토큰별 분기로 합칠 수도 있으나, getter 철자/주소가 달라 manifest 분리가 더 명확.)
- **`self-frozen`** — action / token (transfer·approve·swap). `address` = `$.root.from` (서명자
  자신). 사용자 지갑이 동결 대상이면 그 자산 이동이 revert 하거나 자금이 묶여 있음을 **warn** 으로
  알린다. getter = `(asset)` 분기.
- **`frozen-asset-approve`** — action / token (approve). `address` = `$.action.spender`, 단 frozen
  대상이 spender 든 self 든 "동결 자산에 권한 위임" 자체를 신호 — 위 셋의 일반화. (정확 trigger/severity
  는 정책 작성 시 확정.)

활성화 조건: `/v1/rpc` dispatcher 가 `address.frozen` 을 (위 getter 분기 포함) 서빙하고, 메서드가
`method-catalog.json` 에 등록되고, 위 4 정책의 manifest 가 카탈로그에 추가될 때. 그 전까지 정책은 작성돼
있어도 dormant (field 부재 → guard false → inert).

---

## primary-source references

- [1] Circle USDC `FiatTokenV2` blacklist (`isBlacklisted(address)`, `blacklist`/`unBlacklist`,
  `Blacklister` role): Circle stablecoin (centre-tokens) `FiatToken` 컨트랙트 소스 / Circle 공식 docs —
  https://github.com/circlefin/stablecoin-evm (FiatTokenV1/V2 `Blacklistable`). 함수명 철자
  (`isBlacklisted`, 소문자 list) 는 컨트랙트 ABI 기준. selector hex 는 **출처 미확인** (구현 시 keccak 산출).
- [2] Tether USDT blacklist (`isBlackListed(address)`, `addBlackList`/`removeBlackList`,
  `getBlackListStatus`): Tether (TetherToken) 컨트랙트 소스 — Etherscan verified
  `0xdAC17F958D2ee523a2206206994597C13D831ec7` ABI / Tether transparency 문서. 함수명 철자
  (`isBlackListed`, 대문자 L) 는 컨트랙트 ABI 기준. selector hex 는 **출처 미확인** (구현 시 keccak 산출).
- ERC-20 (`approve`/`transfer`/`transferFrom`/`balanceOf` 기반 의미): EIP-20 —
  https://eips.ethereum.org/EIPS/eip-20.
- EIP-155 (chain id `eip155:<id>` 라우팅 key): https://eips.ethereum.org/EIPS/eip-155.
- 재사용 fetcher (1차 = 코드):
  - `crates/policy-server/sync/src/sources/fetchers/onchain.rs` (`OnchainViewFetcher::fetch_one` /
    `fetch_batch`, `OnchainCall::from_source`).
  - `crates/policy-server/sync/src/sources/fetchers/decoder.rs` (`bool` decoder = `decode_bool`,
    `encode_address`, `function_selector` — 모두 등록·검증됨).
  - `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs` (`Multicall::aggregate3`).
- 와이어 계약 / 프로젝션 제약 / dormancy / selector roots: in-repo
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1, §2, §3a, §5, §6.
- 위 USDC/USDT 컨트랙트 주소 문자열은 참조용이며 1 차 ABI 로 재확인 필요 (proxy 업그레이드로 변동
  가능) — 본 spec 은 주소/셀렉터 hex 를 사실로 단정하지 않는다.
