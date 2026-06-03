# method: approval.allowance

status: existing (in method-catalog.json)

> Implementer-facing spec for the future `/v1/rpc` server. This documents **HOW** to
> build the `approval.allowance` handler, not just its wire shape. Ground truth for the
> wire contract is `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1–3a
> and the catalog policy at
> `crates/policy-engine/tests/fixtures/policy_catalog_v2/action/approval/approve-existing-allowance/`.
> `.md` is gitignored repo-wide, so this file travels as a standalone artifact.

---

## purpose

ERC-20 토큰을 approve 하기 직전, **그 `(owner, asset, spender)` 조합에 이미 0 이 아닌
allowance 가 살아있는지**를 알려준다. dApp 이 기존 allowance 를 0 으로 내리지 않고 곧바로
재-approve 하면 (a) 고전적인 ERC-20 approve front-run race [1] 에 노출되고, (b) 보통 이미
가진 것보다 **더 큰** 권한을 요구한다는 신호다. 정적 calldata 만으로는 "지금 approve 하는
금액" 만 보일 뿐 **체인 현재 상태의 기존 allowance** 는 알 수 없으므로, 이 fact 는 on-chain
`allowance(owner, spender)` view 를 1 회 읽어 enrichment 로 주입해야 한다. 이 fact 가 들어와야
catalog 정책 `approve-existing-allowance` (A6, warn) 이 dormant 에서 깨어난다.

---

## interface

### params (selectors the manifest passes — from the catalog `manifest.json`)

| param | selector | type | 의미 |
|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long (eip155 numeric id) | 어느 체인의 allowance 인지. on-chain read 대상 체인. |
| `owner` | `$.root.from` | String (address) | approve 트랜잭션을 보내는 지갑 = ERC-20 `allowance` 의 `owner`. |
| `asset` | `$.action.token` | AssetRef/String (ERC-20 address) | allowance 를 조회할 토큰 컨트랙트 (`to` of the `eth_call`). |
| `spender` | `$.action.spender` | String (address) | approve 대상 = ERC-20 `allowance` 의 `spender`. |

> `chain_id` 는 셀렉터 상 Long 이지만 라우터는 `ChainId` = `"eip155:<id>"` 문자열로 key 한다
> (`RpcRouter.by_chain: BTreeMap<ChainId, …>`). 핸들러는 `chain_id` 를 받아 `eip155:<id>` 로
> 정규화해서 라우터에 넘긴다.
> `asset` 는 ActionView 상 AssetRef 일 수 있다 — 핸들러는 거기서 ERC-20 `address` 만 추출한다
> (ActionView spelling 의 권위는 `/v1/rpc` planner, POLICY_RPC_METHODS.md §3a "Note on selectors").

### result shape (record returned by the handler)

```json
{
  "allowance": "<uint256 decimal string>",   // String — 현재 raw allowance
  "hasExisting": true,                         // Bool   — allowance != 0
  "isUnlimited": false,                        // Bool   — allowance 가 unlimited sentinel 이상
  "coversRequestedAmount": false               // Bool   — allowance >= requested_amount (요청금액 알 때만 의미)
}
```

- `allowance` : `decode_u256_as_string` 가 내는 십진 문자열 (예 `"0"`, `"115792…"`). hex 아님.
- `hasExisting` : `allowance != 0`. **catalog A6 이 실제로 project 하는 필드.**
- `isUnlimited` : 아래 derivation 의 unlimited 휴리스틱.
- `coversRequestedAmount` : `method-catalog.json` 이 문서화한 기본 scalar return. catalog A6 은
  이걸 쓰지 않으므로 `requested_amount` 가 params 에 없으면 **emit 생략**(아래 dormancy contract).

> **method-catalog.json 과의 gap (의도된 것):** 카탈로그 엔트리는 `returns` 를
> `$.result.coversRequestedAmount → Bool` 로 적지만, catalog 정책 A6 의 manifest 는
> `outputs[].from = "$.result.hasExisting"` 를 project 한다 (POLICY_RPC_METHODS.md §3a 의 †
> gap note). **핸들러는 두 필드를 모두 내보내야 한다** — `hasExisting` (A6 활성화용) 과
> `coversRequestedAmount` (카탈로그 문서 호환용). 둘은 독립적으로 계산되며 서로 fallback 아님.

### projection (record → scalar leaf — MANDATORY)

v2 의 `materialize_v2` 는 record ProjectionType 을 받지 않는다 (`String | Long | Bool | Decimal |
Set<String>` 만, POLICY_RPC_METHODS.md §2). 그러므로 record 는 manifest `outputs[].from` 으로
**반드시 leaf scalar** 까지 내려야 한다.

- catalog A6 projection: `$.result.hasExisting → Bool` (field `hasExistingAllowance`, lowercase
  Cedar `"Bool"`).
- spec 의 1차 권장 leaf (raw value 가 필요한 미래 정책용):
  **`$.result.allowance -> String` (U256 hex/decimal 문자열 leaf)**.
  `allowance` 는 record 가 아니라 scalar string 이어야 한다 — record 를 그대로 project 하면 v2 가
  거부한다.

---

## data source(s)

**EXISTING-FETCHER-REUSABLE.** 새 네트워크 plumbing 0. 다음 building block 을 그대로 재사용:

- `crates/policy-server/sync/src/sources/fetchers/onchain.rs` —
  `OnchainViewFetcher::fetch_one` / `fetch_batch`. `DataSource::OnchainView { chain, contract,
  function, decoder_id }` 로부터 `OnchainCall` 을 만들고 selector + abi-encoded args 를 붙여
  `eth_call` 한 뒤 `decoder_id` 로 returndata 를 decode 한다.
- `crates/policy-server/sync/src/sources/fetchers/decoder.rs` — `erc20_allowance` decoder 가
  **이미 등록**됨 (`DecoderRegistry::with_builtins`, `decode_u256_as_string` 로 alias). selector
  `allowance(address,address)` = `0xdd62ed3e` 도 `function_selector` 테스트로 검증돼 있음.
  args 인코딩은 `encode_address(owner) ++ encode_address(spender)` (각 32-byte left-pad).
- `crates/policy-server/sync/src/sources/fetchers/rpc/router.rs` — `RpcRouter` (체인별 provider
  failover, multicall3 주소 보유). `eth_call(chain, EthCallRequest)`.
- `crates/policy-server/sync/src/sources/fetchers/rpc/providers/public.rs` — `PublicRpcProvider`
  (`eth_call` JSON-RPC). 기본 provider kind = `"public"` (publicnode 등). Alchemy/Infura/QuickNode
  는 `instantiate_provider` 의 주석 stub 상태 (NET-NEW 가 필요하면 그쪽).
- (선택) `rpc/multicall.rs` `Multicall::aggregate3` — 한 액션에 allowance 조회가 ≥2 개일 때
  1 RPC 로 batch (`fetch_batch` 이 자동 사용). 단건이면 `fetch_one` 으로 충분.

> 이 fetcher 들은 **decode-time** `live_inputs` enrichment 용으로 만들어졌다 (POLICY_RPC_METHODS.md
> §5). `/v1/rpc` 메서드 dispatcher 자체는 아직 in-repo 에 없다 — 구현 = `method` 로 keying 하는
> dispatcher 를 만들고, `approval.allowance` 핸들러가 위 fetcher 들을 호출하도록 배선하는 것.

---

## derivation algorithm

입력: `chain_id`, `owner`, `asset`, `spender`, (optional) `requested_amount`.

1. **정규화.** `chain` ← `ChainId("eip155:" + chain_id)`. `owner`/`spender`/`asset` ← 20-byte
   `Address` 파싱 (checksum 무관, lower-case 비교). 셀렉터 누락(`optional:true`)이면 — 아래
   dormancy contract — call 자체를 skip.
2. **calldata 구성.** `function = "allowance(address,address)"`,
   `args = encode_address(owner) ++ encode_address(spender)` (64 bytes). `decoder_id =
   "erc20_allowance"`. `OnchainCall::from_source(DataSource::OnchainView{ chain, contract: asset,
   function, decoder_id }, args)`.
3. **on-chain read.** `OnchainViewFetcher::fetch_one(&call)` → `eth_call(asset, calldata)` at
   `BlockTag::Latest`. (배치면 `fetch_batch` → multicall3.) returndata 32 bytes.
4. **decode.** `erc20_allowance` → `decode_u256_as_string` → `allowance: U256` (십진 문자열).
5. **`hasExisting`** = `allowance != 0`.
6. **`isUnlimited`** (휴리스틱 — 정확하지 않음, 정직하게 명시):
   `allowance >= UNLIMITED_THRESHOLD`. 권장 threshold = `type(uint96).max`
   (`0xFFFFFFFFFFFFFFFFFFFFFFFF`, ~7.9e28) — 실무상 어떤 정상 토큰의 정상 approve 도 이 값을
   넘지 않으면서, `type(uint256).max` (전형적 "infinite approve") 와 Permit2 의 `uint160.max`,
   흔한 dApp 의 큰 approve 를 모두 포함한다. **honest limit:** 토큰 decimals/total-supply 를
   모르므로 절대 정확하지 않다 — decimals 가 매우 큰 토큰의 합법적 대량 approve 를 unlimited 로
   오판할 수 있다. `isUnlimited` 는 **신호용 보조 필드**일 뿐, A6 의 verdict 는 `hasExisting`
   (불확실성 없는 `!= 0` 비교) 으로만 난다.
7. **`coversRequestedAmount`** (요청 금액을 알 때만):
   `requested_amount` 가 params 에 있으면 `allowance >= requested_amount` (U256 비교).
   **없으면 이 필드를 emit 하지 않는다** (default 로 false/true 를 넣지 않는다 — dormancy contract).
   catalog A6 은 `requested_amount` 를 넘기지 않으므로 이 필드는 보통 부재한다.
8. **record 조립** 후 반환. 모든 U256 은 십진 문자열 (`decode_u256_as_string` 와 동일 표현)으로
   직렬화 — v2 String/Bool projection 이 그대로 읽는다.

---

## on-chain calls

- **chain**: `eip155:<chain_id>` — `chain_id` param 에서 도출 (`$.root.chain_id`).
- **contract**: `asset` (ERC-20 토큰 주소, `$.action.token`).
- **view fn**: `allowance(address owner, address spender) returns (uint256)`,
  selector `0xdd62ed3e`.
- **multicall?**: 단건이면 직접 `eth_call`. 같은 체인에서 한 액션에 allowance read 가 ≥2 개면
  `fetch_batch` 가 multicall3 (`aggregate3`, `allow_failure=true`) 로 1 RPC 에 묶는다. 체인 mismatch
  시 batch 거부 (onchain.rs:127-134).
- block tag: `Latest` (현재 상태 — pending mempool 아님, 정적 모델 일관).

---

## caching / ttl

- **cache key tuple**: `(chain_id, asset, owner, spender, blockTag=Latest)`.
- **ttl**: 짧게 — allowance 는 같은 owner 가 다른 트랜잭션으로 언제든 바꾼다. 권장 **≤ 5 s**
  (단일 pre-sign 흐름 내 중복 dispatch 흡수가 목적이지, 장기 캐시 아님). 더 길면 stale allowance 로
  A6 이 잘못 inert/active 될 수 있다.
- **where**: `/v1/rpc` 서버 프로세스 in-memory (per-method LRU/TTL map). 익스텐션 host 의
  `dispatchCallsV2` 는 **per-action fresh results map** 을 유지하므로 (CLAUDE.md "fresh per-action
  results map") cross-action 누수는 서버 측 책임.
- **budget**: orchestrator `HARD_TIMEOUT_MS = 8000`. `PublicRpcProvider` 의 reqwest client 는
  15 s timeout 으로 설정돼 있어 **단독으로는 8 s 예산을 못 지킨다** — 핸들러/dispatcher 는
  per-call deadline 을 8 s 보다 충분히 작게 (예 한 액션 전체 RPC 합 ≤ ~3–4 s) 잡아야 한다. multicall
  batch + provider failover 로 round-trip 1 회면 단순 read 는 보통 수백 ms. cache hit 은 0 RPC.

---

## failure & fallback (DORMANCY CONTRACT)

이 메서드는 **optional** (`policy_rpc[].optional: true`, catalog manifest 가 그렇게 authored).
실패/누락 시 **절대 verdict 를 뒤집을 수 있는 default 를 넣지 않는다.**

- RPC 실패 / decode 실패 / `asset`·`owner`·`spender` 셀렉터 미해결 / 체인 미지원 →
  **해당 output 필드를 emit 하지 않는다** (record 에서 누락 또는 `ok:false`).
- host fold: 필드 부재 ⇒ `context.custom` 에 `hasExistingAllowance` 가 안 들어감 ⇒ Cedar 의
  `context.custom has hasExistingAllowance` 가드가 **false** ⇒ 정책 A6 은 **inert** (절대 거짓
  verdict 없음).
- `requested_amount` 부재 → `coversRequestedAmount` **생략** (false 로 채워서 "안 덮음" 으로 오판
  유도 금지).
- `optional:true` 덕에 dispatcher 가 dormant/unreachable 여도 **missing input → pass 로 degrade**,
  hard batch fail 아님 (POLICY_RPC_METHODS.md §1 "Failure direction").
- **금지**: allowance 를 못 읽었을 때 `hasExisting=false` / `isUnlimited=false` 같은 "안전해
  보이는" default 를 넣는 것. 그건 `!= 0` 인 실제 상태를 가릴 수 있어 A6 을 silently false-pass
  시킨다. 못 읽으면 **field 부재 = 정책 침묵** 이 유일한 올바른 동작.

---

## auth / cost / rate-limit

- **API keys (env)**: 기본 경로(`kind="public"`, 예 `ethereum-rpc.publicnode.com`)는 **키 불필요**.
  provider 설정은 `RpcConfig` (TOML, `[chains."eip155:<id>".providers]`) 로 주입 — `url`/`priority`.
  Alchemy/Infura 등 키 기반 provider 는 `instantiate_provider` stub 을 채워야 하며 그쪽이
  env (`ALCHEMY_API_KEY` 등) 의존 — **NET-NEW**, 기본 동작엔 불필요.
- **per-call cost**: `allowance` 1 read = `eth_call` 1 회 (또는 multicall 로 N read = 1 회).
  무상태·저비용. cache hit 은 0 네트워크.
- **rate-limit**: 공개 RPC 는 IP 기반 rate-limit 이 있을 수 있음. `RpcRouter` 의 provider failover
  (`try_all` + `HealthTracker`) 가 한 provider 가 throttle/down 일 때 다음 priority 로 넘긴다.
  ≤5 s TTL 캐시가 같은 pre-sign 흐름의 중복 조회를 흡수해 호출량을 줄인다.

---

## activation

이 메서드를 `/v1/rpc` dispatcher 가 (위 `hasExisting` 필드 포함) 서빙하면 다음 dormant catalog
정책이 깨어난다:

- **`approve-existing-allowance`** — action / approval, severity `warn`.
  trigger `action.tag == "erc20_approve"`; project `$.result.hasExisting → Bool`
  (`context.custom.hasExistingAllowance`); fires when `hasExistingAllowance == true`.
  (POLICY_RPC_METHODS.md §4 activation map: `approval.allowance` → 1 policy.)

---

## primary-source references

- [1] ERC-20 `approve` race / front-run: EIP-20 (`approve(address,uint256)` 명세, "the
  attack vector" 경고가 따라붙는 표준 패턴) — https://eips.ethereum.org/EIPS/eip-20.
  catalog A6 의 cedar 주석이 인용하는 "ERC-20 approve front-run" / "revoke-to-zero first" 권고와
  동일 근거.
- ERC-20 `allowance(address,address) returns (uint256)` view 의미: EIP-20 — 위 동일 출처.
- selector `0xdd62ed3e` = `keccak256("allowance(address,address)")[..4]`: in-repo 검증
  (`decoder.rs` 테스트 `selectors_match_known`). EIP-20 ABI 기준.
- Multicall3 `aggregate3` ABI / `0x82ad56cb`: in-repo `rpc/multicall.rs` 모듈 doc-comment.
  (Multicall3 표준 명세 자체의 1차 URL 은 **출처 미확인** — 코드의 ABI 주석으로 grounding.)
- unlimited-approve 휴리스틱 (`uint256.max` infinite approve, revoke 권고): Revoke.cash /
  approval-phishing 운영 관행에 근거 — POLICY_RPC_METHODS.md §7 provenance 목록(Revoke.cash,
  Chainalysis approval-phishing) 참조. 본 spec 의 `uint96.max` threshold 수치는 **구현 선택**
  (1차 표준 아님) — 보수적 신호용 보조값으로만 사용, verdict 는 `hasExisting` 으로 결정.
- 와이어 계약 / 프로젝션 제약 / dormancy: in-repo
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1–5, §7.
