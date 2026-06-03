# method: portfolio.balance

status: existing (in method-catalog.json)

> 이 파일은 미래의 `/v1/rpc` 서버 구현자가 읽는 **구현 스펙**이다. wire interface 만이 아니라
> *어떻게* 만드는지를 기술한다. wire 계약/projection 제약/활성화 정책 목록의 1차 출처는
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3a) 이며, 메서드
> 카탈로그 엔트리의 1차 출처는 `schema/method-catalog.json` `portfolio.balance` (`returns.kind:"scalar"`,
> `from:"$.result.balance"`), 재사용 가능한 plumbing 의 1차 출처는
> `crates/policy-server/sync/src/sources/fetchers/{onchain,decoder,rpc}` 코드다. 호출처/투영의 SSOT 는
> 이 메서드를 부르는 2개 catalog manifest (아래 activation) 다. 모든 진술은 해당 코드/문서에 grounding
> 되어 있고, 검증 못 한 것은 "출처 미확인" 으로 명시했다.

## purpose

actor(`from`) 가 **특정 토큰을 on-chain 으로 얼마나 보유**하는지(raw U256 잔고)를 가져와
`context.custom.balance` 로 주입한다. 정적 calldata 디코드는 "이 transfer 금액이 보유 잔고와
같은지(전량 sweep)" 나 "이 토큰을 애초에 보유하긴 하는지(0 잔고)" 를 알 수 없다 — 잔고는 체인
상태이지 calldata 필드가 아니다. 이 enrichment 가 `balanceOf(owner)` 한 번을 읽어 정책이 비교할
수 있게 한다.

`portfolio.input_fraction_bps` 와의 차이: 후자는 잔고를 **분모로만 써서 비율(bps)** 로 환원해 내보낸다.
이 메서드는 **잔고 raw 값 자체** 를 그대로 내보낸다 — 정책이 `amount == balance`(전량) 또는
`balance == "0x0"`(미보유) 같은 **정확한 동등 비교**를 하기 때문이다(아래 표현형 주의). ScopeBall 의
no-simulation 모델과 일관되게 이것은 transaction trace 가 아니라 **단일 view read** 다.

## interface

고정 — `schema/method-catalog.json → methods["portfolio.balance"]` + 호출 manifest 2개 그대로.

### params (각 `$.`-selector + type)

| param | type | required | defaultSelector | 설명 |
|---|---|---|---|---|
| `chain_id` | `Long` | yes | `$.root.chain_id` | 잔고를 읽을 chain (EIP-155 chain id, 예: `1` = ethereum). |
| `owner` | `String` | yes | `$.root.from` | 잔고 주인 = tx 서명자(0x-주소). |
| `asset` | `AssetRef` | yes | `$.action.inputToken.asset` | 잔고를 셀 토큰. address/standard 를 담은 ref. |

**호출 manifest 의 실제 selector** (catalog 의 transfer-shaped caller — swap-shaped default `$.action.inputToken.asset`
와 다름; 두 caller 모두 동일):
```json
"params": {
  "chain_id": "$.root.chain_id",
  "owner":    "$.root.from",
  "asset":    "$.action.token"
}
```
→ planner 는 두 selector 형태(`$.action.inputToken.asset` swap / `$.action.token` transfer)를
모두 resolve 할 수 있어야 한다. 같은 메서드가 caller 모양에 따라 다른 selector 로 호출된다. 이
`$.action.*` selector 는 **ActionView** JSON 에 대해 resolve 되며, 그 spelling 의 authority 는
`/v1/rpc` planner 다(`POLICY_RPC_METHODS.md` §3a non-swap caller note). 구현자는 manifest 가 넘기는
selector 를 신뢰하고, 자기 param 타입만 맞추면 된다.

### result shape (record fields + types)

`method-catalog.json` 의 `returns` 는 `{ "kind": "scalar", "type": "String", "from": "$.result.balance" }`
— 이미 scalar 메서드다(record 가 아님). 호스트는 `$.result.balance` 를 갖는 객체를 돌려준다:

| field | type | 설명 |
|---|---|---|
| `balance` | String (U256, **hex `0x…` form**) | **필수.** actor 의 해당 토큰 raw 잔고. 정책이 읽는 유일한 leaf. 표현형은 아래 "표현형 계약" 참조. |

> 추가 디버그 필드(예: `decimals`, `balanceDecimal`)를 더 실어도 무방하나 **projection leaf 는
> `balance` 하나** 다 — 다른 필드는 정책 verdict 에 영향이 없다(scalar 만 투영되므로). 구현 1차
> 컷은 `balance` 만 채워도 된다.

### projection: `$.result.balance → String` (scalar leaf — **mandatory**)

`POLICY_RPC_METHODS.md` §2 의 hard 제약: `materialize_v2` 는 **scalar** projection type 만
`context.custom.*` 에 받는다(`String | Long | Bool | Decimal | Set<String>`). 이 메서드는 이미
`String` scalar 라 record→leaf 강등이 없지만, manifest 가 명시적으로:
```json
"outputs": [{ "kind":"context", "field":"balance", "type":"String", "from":"$.result.balance", "required":false }],
"custom_context": { "fields": { "balance": "String" } }
```
로 투영한다. `outputs[].field` ⇄ `custom_context.fields` 는 1:1(`ManifestV2::validate` 강제).
`custom_context` 철자는 lowercase Cedar(`"String"`). 즉 정책이 실제로 보는 것은
`context.custom.balance : String` 하나다.

### 표현형 계약 (hex U256 — **구현자 필독, 비자명**)

호출 정책 2개는 `balance` 를 **숫자가 아니라 hex String 으로 정확 동등 비교**한다(코드 검증):

- `transfer-full-balance-sweep` (policy.cedar): `context.amount == context.custom.balance`.
- `transfer-token-not-held-warn` (policy.cedar): `context.custom.balance == "0x0"`.

여기서 `context.amount` 는 ActionBody lowering 이 **`u256_hex` = `format!("{v:#x}")`** 로 만든
hex String 이다(`crates/policy-engine/src/lowering_v2/common/cedar.rs:6` `u256_hex`,
`lowering_v2/token/erc20_transfer.rs:26` `m.insert("amount", u256_hex(action.amount))`). 이 표현형은:

- `0x` 접두 + lowercase hex, **leading-zero 패딩 없음**(alloy `LowerHex`). 따라서 **zero = `"0x0"`**
  (정책의 `"0x0"` sentinel 과 정확히 일치).
- 즉 `balance` 도 **반드시 같은 `{:#x}` 표현**으로 내보내야 정책의 String 동등이 성립한다.

**함정:** 재사용 디코더 `erc20_balance`(`decoder.rs:57` → `decode_u256_as_string`)는 잔고를
**decimal** String(`v.to_string()`)으로 돌려준다(`decoder.rs:84`). 이 raw decimal 을 그대로 투영하면
정책의 hex 비교(`== context.amount`, `== "0x0"`)가 **항상 false** 가 되어 정책이 silent 하게
inert 된다(spurious pass — 함정). 따라서 이 메서드의 NET-NEW 작업에 **decimal→`{:#x}` 재렌더**가
포함된다(아래 derivation 4단계). decimal 그대로 투영 금지.

## data source(s)

분자/분모 없는 단일 view read — **on-chain `balanceOf` read** 만. 외부 가격 API/인덱서 불필요
(USD 환산도, 비율도 아니고 raw 잔고이므로 decimals 도 불필요).

### ERC-20 잔고 read — EXISTING-FETCHER-REUSABLE: `OnchainViewFetcher`

- **Fetcher**: `OnchainViewFetcher`
  (`crates/policy-server/sync/src/sources/fetchers/onchain.rs`).
- 무엇을 주는가: `OnchainCall::from_source(DataSource::OnchainView{ chain, contract=asset.address,
  function:"balanceOf(address)", decoder_id:"erc20_balance" }, encode_address(owner))` 로
  selector `0x70a08231` + 32-byte owner 를 합성(`decoder.rs:function_selector`/`encode_address`,
  테스트 `from_source_with_args` 가 정확히 이 모양을 검증). `fetch_one`(단건) 또는
  `fetch_batch`(multicall3 `aggregate3`, `allow_failure=true`)로 `eth_call`.
- 재사용 방식: 디코더 `"erc20_balance"` 가 **이미 등록**됨(`decoder.rs:57` →
  `decode_u256_as_string`) → returndata 32-byte word 를 U256 로 읽어 **decimal String** 으로 반환.
  NET-NEW 디코더 불필요.

### Native asset 잔고 read — NET-NEW (소량, 현재 미exercise)

- `asset` 이 `{"standard":"native",...}` 이면 ERC-20 컨트랙트가 없으므로 `eth_getBalance(owner,"latest")`
  (calldata 없는 RPC). `RpcRouter` 에 `eth_call` 은 있으나 `eth_getBalance` 헬퍼는 별도 추가 필요.
  **현재 호출 manifest 2개는 `erc20_transfer` 트리거뿐이라 erc20 경로만 exercise 된다**; native
  분기는 미래 native-transfer caller 용으로 명세만 두고, 미구현이면 dormancy contract(아래)로
  field 를 drop — fabricated `"0x0"` 으로 채우지 않는다(0 잔고 = `transfer-token-not-held-warn`
  warn 을 spurious 하게 트리거하므로 절대 금지).

### NET-NEW (이 메서드 고유)

- `/v1/rpc` method dispatcher 자체(오늘 repo 에 **없음** — `POLICY_RPC_METHODS.md` §5).
  `policy-server` `handler.rs` 의 `results = BTreeMap::new()` 는 `/evaluate` 시뮬레이션 경로지
  이 enrichment endpoint 가 아니다.
- `AssetRef`(`TokenKey`, `#[serde(tag="standard")]`) → (erc20 `balanceOf` | native `eth_getBalance`
  | non-fungible drop) 분기 라우팅.
- **decimal→hex 재렌더**: `erc20_balance` 디코더의 decimal String 을 `U256::from_dec_str(...)` 로
  파싱 후 `format!("{v:#x}")`(`u256_hex` 와 동일 형식)로 재출력. 이 변환이 표현형 계약의 핵심.

## derivation algorithm

입력: `chain_id`(Long), `owner`(String, 0x-주소), `asset`(TokenKey).

1. **asset 분기**(`TokenKey` `standard` 태그로):
   - `erc20` → 2번으로.
   - `native` → `eth_getBalance(owner,"latest")` 로 U256 잔고 → 4번으로(NET-NEW, 현재 미exercise).
   - `erc721`/`erc1155` 등 non-fungible → "한 토큰의 fungible 잔고" 가 정의 불가 → **field drop**
     (dormancy). emit 안 함.
2. **잔고 read**(erc20): `balance_dec = OnchainViewFetcher.fetch_one(balanceOf(owner) @ asset.address
   @ chain)` → decimal String. read 실패(RPC error / revert / `!success`) → **field drop**(dormancy).
   **`"0x0"`/`0` 으로 대체 금지.**
3. **U256 파싱**: `bal = U256::from_dec_str(balance_dec)`. 파싱 실패(이론상 디코더가 항상 정수
   String 을 주므로 거의 없음) → field drop.
4. **hex 재렌더**: `balance_hex = format!("{bal:#x}")` (`u256_hex` 와 byte-identical 형식;
   zero → `"0x0"`). **이 값이 projection leaf.** decimal 그대로 투영 금지(표현형 계약 참조).
5. `{"balance": balance_hex}` 반환. host fold 가 `$.result` payload(즉 `{"balance":"0x…"}`)를
   `map[call_id]` 에 넣고, engine 이 `$.result.balance → String` 을 뽑아 `context.custom.balance`
   로 materialize.

### heuristic limits (정직한 한계)

- **시점 차이**: `balanceOf` 는 `latest` 블록 시점이고 tx 는 아직 미체결. mempool 의 다른 pending
  tx(또는 같은 액션 앞 leg)가 잔고를 바꿀 수 있어 read 잔고는 **서명 시점 근사**다. 두 호출 정책은
  거친 동등(전량/0)만 보지만, 엄밀한 `amount == balance` 동등은 그 사이 잔고 변동에 민감하다 —
  drainer-sweep 탐지가 1 wei 차이로 miss 될 수 있음(false negative 방향, fail-safe).
- **fee-on-transfer / rebasing 토큰**: `amount`(calldata 명목값)와 실제 차감액이 다를 수 있어
  `amount == balance` 가 어긋날 수 있음(그 류는 `token.metadata` 메서드의 영역). 이 메서드는
  read 한 raw 잔고만 정직히 보고한다.
- **proxy/non-standard ERC-20**: `balanceOf` 가 표준과 다르게 동작하는 토큰(예: share 기반
  rebasing)은 read 값이 사용자 직관과 어긋날 수 있다. 디코더는 returndata 32-byte 를 U256 로
  읽을 뿐 의미 검증은 안 한다.
- **AssetRef 표준 커버리지**: erc20·native 외 standard 는 drop → 그 자산엔 정책 INERT. 과장 금지:
  현재 구현 커버리지가 곧 이 메서드의 커버리지다.

## on-chain calls

erc20 경로(현재 호출되는 유일 경로):

- **chain**: `eip155:<chain_id param>` (예: `1` → `eip155:1`). `RpcRouter` 가 chain 별 provider 로
  라우팅.
- **contract**: `asset.address` (ERC-20 토큰 컨트랙트).
- **view fn**: `balanceOf(address)` (selector `0x70a08231`), arg = `owner` (32-byte left-padded).
- **multicall?**: 단일 call 이라 불필요하나, 같은 action 의 다른 on-chain enrichment(예
  `approval.allowance` / `portfolio.input_fraction_bps`)와 **같은 chain 이면
  `OnchainViewFetcher.fetch_batch` 의 multicall3 `aggregate3` 로 묶어 RTT 1 회로 합치는 것을 권장**
  (`allow_failure=true` 이므로 한 leg 실패가 다른 leg 를 죽이지 않음 — 실패 leg 는 그 field 만 drop).

native 경로: `eth_getBalance(owner,"latest")` (calldata 없음, multicall3 불가 — 별도 RPC).

## caching / ttl

- **cache key**: `(chain_id, owner, asset_key)`. value = read 한 raw 잔고(hex String 또는 U256).
  amount 와 무관(이 메서드는 amount param 자체가 없음).
- **ttl**: 짧게 — **2~5 s** 권장(잔고는 빠르게 변하고, 한 사용자의 연속 액션 approve→transfer
  내에서만 재사용하면 충분). per-action results map(call_id 반복 시 fresh map) 컨벤션과 충돌 없음.
  **출처 미확인**(이 TTL 수치는 본 repo 코드에 명시 상수가 없는 권장값).
- **where**: `/v1/rpc` 서버 in-process LRU(host 측). 익스텐션 `dispatchCallsV2` 는 캐시하지 않음.
- **budget**: orchestrator `HARD_TIMEOUT_MS = 8000`. 단일 `eth_call`(또는 batch leg) 1 RTT
  (~100–500 ms public RPC) → 여유. cache hit 시 0 RTT. 같은 토큰에 두 호출 정책
  (sweep + not-held)이 동시 활성이면 둘이 **동일 `(chain,owner,asset)` call_id** 를 쓰므로 잔고
  read 가 1 회로 흡수된다(plan 단계에서 dedup; 같은 plan key 이면 캐시가 흡수). 느린 RPC 가 batch
  전체를 8 s 까지 끌지 않도록 **per-call 타임아웃**(예: 2–3 s)을 두고 초과 시 그 leg 만 drop.

## failure & fallback (DORMANCY CONTRACT)

이 메서드의 두 catalog caller 는 `policy_rpc[].optional: true` + `outputs[].required: false`
(manifest 확인됨). 따라서 fail-safe 방향이 강제된다:

- 어떤 실패(RPC error, revert, `!success`, U256 파싱 실패, native 미구현, non-fungible asset,
  per-call timeout, param selector 미해소) → **`balance` field 를 emit 하지 않는다**(또는 result 를
  통째로 비운다).
- host fold 는 missing/`ok:false` 결과를 `map[call_id]` 에서 **drop** 한다
  (`POLICY_RPC_METHODS.md` §1 wire contract).
- 그 결과 `context.custom` 에 `balance` 필드가 **없음** → 두 정책 모두 `context.custom has balance`
  guard 가 **false** → `forbid` when-clause 전체가 false → **정책 INERT**(warn 미생성). dormant
  정책은 false verdict 를 만들지 않는다.
- **절대** verdict 를 뒤집을 수 있는 default 를 대입하지 않는다. 특히:
  - `balance = "0x0"` default 금지 → `transfer-token-not-held-warn` 을 **항상 warn** 으로 false-fire.
  - 임의 큰 값/임의 비-amount 값 금지 → `transfer-full-balance-sweep` 동등을 깨거나 spurious 발화.
  - decimal String 그대로 emit 금지(표현형 계약) → 두 정책 hex 비교가 항상 false → silent inert
    (spurious pass). 모르면 비운다, 추측을 fact 로 둔갑시키지 않는다.
- `optional: true` 이므로 이 한 call 의 실패가 batch 전체를 hard-fail 시키지 않는다 → 그 action 은
  다른 정책 결과대로 자연 진행(degrade to pass for *this* policy, never a spurious block).
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. fail-closed 방향(deny)
  으로 흐르지 않는다(enrichment 의 일반 계약; deny-closed HyperLiquid 경로와 무관 — 이 메서드는
  wallet-안전 enrichment 일 뿐 venue 차단 로직이 아니다).

## auth / cost / rate-limit

- **API keys (env)**: 외부 price/indexer API 불필요. 필요한 것은 chain RPC endpoint 뿐 —
  `RpcRouter` 의 provider 설정(public RPC 또는 chain 별 `*_RPC_URL` env, 서버 RPC config 소관).
  별도 per-method 키 없음.
- **per-call cost**: `eth_call` 1 회(캐시 miss 시). 무료 public RPC 또는 archive-불요 `latest`
  read. native 는 `eth_getBalance` 1 회.
- **rate-limit**: public RPC 의 RPS 제한이 유일 제약. 같은 `(chain,owner,asset)` 반복 액션은
  TTL 캐시가 흡수; 같은 action 내 동일 chain enrichment 는 multicall3 로 1 RTT 합산; 같은 토큰의
  두 호출 정책은 잔고 read 1 회 공유. burst 시 per-call timeout 으로 8 s budget 보호.

## activation

이 메서드를 `/v1/rpc` 가 서빙하면 dormant 해제되는 catalog policy 2개
(`POLICY_RPC_METHODS.md` §4 — 다만 본 §4 activation map 표는 작성 시점에 이 2개를 별도 행으로
열거하지 않음; 호출 manifest 의 `methods: portfolio.balance` 주석이 1차 ground-truth):

- **`transfer-full-balance-sweep`**
  (`action/transfer/transfer-full-balance-sweep`, catalog action/transfer) —
  `forbid` on `Token::Action::"Erc20Transfer"` when `context.amount == context.custom.balance`,
  `@severity("warn")`. transfer 금액이 보유 잔고 전량과 같은 drainer-style sweep 을 서명 전 warn.
- **`transfer-token-not-held-warn`**
  (`wallet/cooldown/transfer-token-not-held-warn`, catalog wallet/cooldown) —
  `forbid` on `Token::Action::"Erc20Transfer"` when `context.custom.balance == "0x0"`,
  `@severity("warn")`. 보유 0(미보유/phantom·dust-airdrop scam 토큰) 토큰 전송을 서명 전 warn.

> `portfolio.balance` 는 `method-catalog.json` 에 `origin:"bundled"` 로 **이미 등록**되어 있다
> (registered method — `transfer-token-not-held-warn` policy.cedar 주석 "Registered method — active
> (not dormant)"). 단 §5 대로 `/v1/rpc` dispatcher 본체가 없으므로, dispatcher 가 생기기 전까지는
> 다른 enrichment 와 동일하게 실효 dormant 다(field 부재 → guard false). dispatcher + 이 핸들러를
> 구현하면 두 정책이 라이브된다.

## primary-source references

- ScopeBall enrichment wire 계약 / projection 제약 / non-swap selector note:
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1, §2, §3a, §5) — repo 내부 1차.
- 메서드 카탈로그 엔트리(params/returns scalar/from/origin): `schema/method-catalog.json`
  `methods["portfolio.balance"]`.
- 호출처/투영 SSOT(repo-internal, 1차):
  - `crates/policy-engine/tests/fixtures/policy_catalog_v2/action/transfer/transfer-full-balance-sweep/{manifest.json,policy.cedar}`.
  - `crates/policy-engine/tests/fixtures/policy_catalog_v2/wallet/cooldown/transfer-token-not-held-warn/{manifest.json,policy.cedar}`.
- 표현형 계약(amount/balance hex String):
  - `crates/policy-engine/src/lowering_v2/common/cedar.rs` (`u256_hex` = `format!("{v:#x}")`).
  - `crates/policy-engine/src/lowering_v2/token/erc20_transfer.rs` (`amount` = `u256_hex(action.amount)`).
- 재사용 fetcher / 디코더 (1차 = 코드):
  - `crates/policy-server/sync/src/sources/fetchers/onchain.rs` (`OnchainViewFetcher` / `OnchainCall::from_source` / `fetch_one` / `fetch_batch`).
  - `crates/policy-server/sync/src/sources/fetchers/decoder.rs` (`function_selector` / `encode_address` / `decode_u256_as_string` / `erc20_balance` 등록 — **decimal** String 출력).
  - `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs` (`Multicall::aggregate3`).
- 외부 표준 출처:
  - ERC-20 `balanceOf(address)` (selector `0x70a08231`) — EIP-20, "Methods" section.
    https://eips.ethereum.org/EIPS/eip-20
  - `eth_call` / `eth_getBalance`(`"latest"` block tag, native 잔고 경로) — Ethereum JSON-RPC API spec.
    https://ethereum.org/en/developers/docs/apis/json-rpc/
  - EIP-155 (chain id) — https://eips.ethereum.org/EIPS/eip-155.
  - Multicall3 `aggregate3` 표준 컨트랙트(같은-chain enrichment 묶음) — https://github.com/mds1/multicall
    (1차 GitHub spec); repo 내 SSOT 는 `fetchers/rpc/multicall.rs`.
- TTL 권장 수치(2~5 s)는 repo 명시 상수 없음 → **출처 미확인**(권장값).
