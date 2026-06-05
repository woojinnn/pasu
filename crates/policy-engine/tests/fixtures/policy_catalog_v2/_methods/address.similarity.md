# method: address.similarity

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> Implementer-facing build spec for the `/v1/rpc` enrichment method `address.similarity`.
> Interface-level description lives in
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (§1 wire contract, §2
> projection, §6 conventions). The 1차 출처 for the *exact* wire shape this method must satisfy is
> the consumer manifest at
> `crates/policy-engine/tests/fixtures/policy_catalog_v2/action/transfer/transfer-address-poisoning/manifest.json`.
> This file is the **HOW** (derivation, plumbing, caching, failure contract); the manifest +
> `POLICY_RPC_METHODS.md` are the **WHAT** (wire shape). They must stay consistent — if you rename a
> param/result field here, change it in the manifest too.

## purpose

Address-poisoning(주소 오염) 공격은 공격자가 **피해자의 알려진 counterparty 주소와 앞/뒤 hex
글자가 일치하는 vanity 주소**를 만든 뒤, 그 주소에서 dust / zero-value 전송을 피해자에게 흩뿌려
지갑 히스토리에 "비슷해 보이는" 한 줄을 심어 두는 수법이다. 피해자가 다음 전송 때 히스토리에서
오염된 항목을 복사해 붙이면, 진짜 counterparty 가 아니라 공격자에게 자금이 가고 — 회수 불가능하다
(Source: Etherscan / MetaMask address-poisoning advisories).

정적 calldata 만으로는 이 충돌을 알 수 없다. 디코더는 "수령 주소 `candidate`" 라는 사실만 가질
뿐, **그 주소가 사용자의 과거 counterparty 중 하나와 시각적으로 충돌하지만 실제로는 그 중 하나가
아니다** 라는 판단은 지갑 히스토리(known counterparty 집합)와의 비교가 있어야 가능하다.
`address.similarity` 는 `candidate` 를 사용자의 known-counterparty 집합과 대조해 **lookalike 충돌
여부 1건(`poisonCollision: Bool`)** 을 enrich 한다. 1차 소비자는 카탈로그 정책
`transfer-address-poisoning` (action `transfer`, severity `warn`) 으로, `poisonCollision == true`
인 ERC-20 전송을 `warn` 시킨다. ScopeBall 의 no-simulation 모델과 일관되게 이 메서드는 트랜잭션
트레이스가 아니라 **로컬 집합 비교 1건** 이다 (네트워크 fetch 조차 없다 — 아래 data source 참조).

## interface

(Source of truth for the wire shape: the consumer `manifest.json` cited above + `POLICY_RPC_METHODS.md`
§§1–2. Reproduced here; do not diverge.)

### params (`$.`-selectors)

| param | selector (manifest) | type | required | note |
|---|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long | yes | EIP-155 chain id (e.g. `1`). counterparty 집합·주소 normalize 의 scope key. |
| `candidate` | `$.action.recipient` | String | yes | 검토 대상 수령 주소(0x-hex, 20 bytes). `transfer-address-poisoning` 에선 transfer recipient. planner 가 ActionView spelling(`$.action.recipient`)의 authority. |
| `known_counterparties` | — (manifest 에 **없음**) | Set\<String\> | no (host-supplied) | 사용자가 과거에 상호작용한 counterparty 주소 집합. **manifest param 이 아니다** — 매니페스트는 `chain_id`+`candidate` 만 넘긴다. 이 집합은 `/v1/rpc` 서버가 host-side 지갑 히스토리 store 에서 `(chain_id, $.root.from)` 로 자기가 채운다(아래 data source). 즉 정책이 주는 입력이 아니라 host 가 갖고 있는 컨텍스트다. |

`call_id = "<manifest_id>::<spec_id>"` (예: `transfer-address-poisoning::recipient-similarity`) per §1 wire contract.

> **왜 `known_counterparties` 가 manifest param 이 아닌가.** v2 selector 는 디코드된 ActionBody/
> ActionView 만 가리킬 수 있다(§6). 지갑 히스토리는 디코드 산출물이 아니라 host 가 별도로 보유한
> 상태이므로 `$.action.*` 로 투영할 수 없다. 따라서 매니페스트는 `candidate` 만 넘기고, 비교 대상
> 집합은 **서버가 owner(`$.root.from`)로 자기 store 를 조회해 주입**한다. 구현자는 매니페스트가
> 넘기는 두 param 만 신뢰하고, 집합은 서버 내부에서 해소한다.

### result shape (record)

```json
{ "poisonCollision": <Bool>, "lookalikeOf": <String|absent> }
```

| field | type | required | meaning |
|---|---|---|---|
| `poisonCollision` | Bool | yes | `true` iff `candidate` 가 known counterparty 중 하나와 prefix/suffix lookalike 로 충돌하지만 **그 counterparty 와 동일하지는 않다**. 정책이 읽는 유일한 leaf. |
| `lookalikeOf` | String | **optional** | 충돌한 known counterparty 주소(어느 진짜 주소를 흉내냈는가). reason/debug 표면화용. 없어도 정책 verdict 에 영향 없음 — 1차 컷은 `poisonCollision` 만 채워도 된다. |

### projection (record → scalar leaf — MANDATORY)

v2 `materialize_v2` 는 **scalar** projection type(`String | Long | Bool | Decimal | Set<String>`)만
`context.custom.*` 에 받는다 — record ProjectionType 은 없다(§2). 따라서 record 를 반환하더라도
매니페스트가 `outputs[].from` 으로 leaf scalar 까지 투영해야 한다. 소비자 매니페스트가 박아둔 투영:

| `outputs[].from` | `outputs[].type` | `custom_context` field (lowercase Cedar) | consumer |
|---|---|---|---|
| `$.result.poisonCollision` | `Bool` | `"Bool"` | `transfer-address-poisoning` (THE leaf it tests) |

`outputs[].field ⇄ custom_context.fields` 는 1:1 (`ManifestV2::validate` 강제). 매니페스트 실측값:
`custom_context.fields = { "poisonCollision": "Bool" }`, `outputs[0].required = false`, `optional: true`.
즉 host 가 어떤 모양을 돌려주든 정책이 실제로 보는 것은 `context.custom.poisonCollision : bool` 하나다.
`lookalikeOf` 는 현재 어떤 카탈로그 정책도 투영하지 않으므로(매니페스트에 outputs 엔트리 없음) 채워도
verdict 에 무영향 — 미래 reason-surfacing 정책용 예약 필드다.

## data source(s)

**NET-NEW — 전부.** 가격/온체인 reuse 가 없다. 이 메서드는 (1) host-side 지갑 히스토리 store 와
(2) 순수 문자열 비교 휴리스틱 두 조각으로 구성되며, **둘 다 in-repo 에 존재하지 않는다**.

- **counterparty 집합 source — NET-NEW (in-repo 부재 확인됨).**
  사용자가 과거 상호작용한 counterparty 주소 집합은 host(extension/서버)의 **지갑 히스토리 레이어**
  에서 와야 한다. 이 task 의 설계 의도는 sync Orchestrator 의 **wallet store** 를 통해 `(chain_id,
  owner=$.root.from)` 로 counterparty 집합을 조회하는 것이다. **단, 그런 store/fetcher 는 현재 repo
  에 없다:** `crates/policy-server/sync/src/sources/` 는 `discovery / fetchers / mod.rs /
  primitives.rs / subscription.rs` 만 있고, `fetchers/` 하위는 `oracle / onchain / registry / venue
  / rpc / abi_decoder` 뿐 — **account/counterparty history index 는 0개**다(repo grep 으로
  `counterpart|wallet_history|address_book|known_address|recipient_history` 0건 확인). 따라서
  counterparty 집합 확보는 **온전히 NET-NEW** 이고, 다음 중 하나로 구현해야 한다:
  - host(extension) 가 자기 transaction-history(이미 UI 가 보유)에서 owner 의 counterparty 집합을
    뽑아 `/v1/rpc` 호출에 동반 전달하는 사이드 채널, 또는
  - 서버가 explorer/indexer(예: Etherscan `account` txlist)로 owner 의 outbound 수령자 목록을
    재구성하는 NET-NEW fetcher. (이쪽은 `address.activity` Tier-2 와 같은 indexer 의존성 — 비용·
    rate-limit·키 이슈가 동일하게 따라온다.)
  어느 쪽이든 **본 repo 에 재사용 가능한 fetcher 가 없다.**
- **lookalike 비교 휴리스틱 — NET-NEW, 순수 로컬 산술(I/O 없음).**
  집합만 있으면 충돌 판정은 **네트워크 없이** 가능하다. `candidate` 와 각 known counterparty 의
  0x-hex 문자열을 lowercase normalize 한 뒤, **앞 N글자 + 뒤 M글자 일치** 같은 prefix/suffix
  휴리스틱으로 "닮았지만 같지 않음" 을 판정한다. 이 부분은 순수 산술이므로, 만약 counterparty 집합을
  host 가 호출에 동반 전달하는 설계를 택하면 이 메서드는 **`local-method-handlers.ts`
  (`tryHandleLocally`) 에서 in-process 로 답할 수 있다** — `/v1/rpc` POST 조차 불필요(§1 local-first,
  §6 local handlers). 반대로 서버가 indexer 로 집합을 재구성해야 하면 I/O 가 끼므로 서버 핸들러에
  둔다. **구현 결정은 "counterparty 집합을 어디서 얻는가" 에 종속된다.**

> 요약: **온체인 read 없음, 가격 oracle 없음, 기존 fetcher reuse 없음.** 유일한 외부 의존은
> "counterparty 집합을 어떻게 조달하느냐" 하나이며 그조차 in-repo 부재 → 전부 NET-NEW.

## derivation algorithm

입력: `chain_id` (Long), `candidate` (String), `known_counterparties` (Set\<String\>, host 가 해소).

1. **normalize.** `candidate` 를 lowercase 0x-hex 로 정규화하고 20-byte 길이 검증(malformed → failure path).
   known counterparty 주소들도 동일하게 lowercase normalize.
2. **counterparty 집합 확보.** `(chain_id, owner=$.root.from)` 로 host wallet store(또는 동반 전달
   집합)에서 known counterparty 집합 `K` 를 얻는다. `K` 가 비었거나 조회 불가 → **abort**(결과 없음 →
   dormancy contract). 집합 커버리지가 곧 이 메서드의 커버리지다(과장 금지).
3. **exact-match 제외.** `candidate ∈ K` 이면 그건 *진짜* counterparty 다 → `poisonCollision = false`
   (오염이 아님). 이 가지는 명시적으로 "충돌 아님" 으로 끝낸다 — lookalike 의 정의가 "닮았지만 **같지
   않음**" 이므로 자기 자신과의 일치는 충돌이 아니다.
4. **lookalike 스캔.** `candidate` 와 다른 각 `k ∈ K` 에 대해 prefix/suffix 휴리스틱 적용:
   - `prefix_len` = `candidate` 와 `k` 의 선두 일치 hex 글자 수(`0x` 제외 후 비교 권장).
   - `suffix_len` = 말미 일치 hex 글자 수.
   - **충돌 판정 임계값**: `prefix_len ≥ P` **그리고** `suffix_len ≥ S` (전형적 vanity-poisoning 은
     앞 4글자 + 뒤 4글자를 맞춘다 — 권장 시작값 `P = S = 4`, 즉 앞 4 + 뒤 4 hex nibble 일치). 임계값은
     튜닝 파라미터이며 이 repo 에 명시 상수가 없다 — **권장값, 출처 미확인**. 너무 낮으면 false
     positive(무관한 주소가 우연히 4글자 겹침), 너무 높으면 공격을 놓침 → false negative.
   - 매칭되는 `k` 가 하나라도 있으면 `poisonCollision = true`, `lookalikeOf = k`(가장 강한 매칭
     하나; 여러 개면 prefix_len+suffix_len 최대인 것).
5. **조립.** `{ poisonCollision, [lookalikeOf] }` 를 unwrapped `$.result` payload 로 반환. 매니페스트
   투영 `$.result.poisonCollision → Bool` 이 `poisonCollision` leaf 를 뽑는다.

**heuristic limits (정직한 한계):**
- **순수 시각적 prefix/suffix 휴리스틱.** 실제 address-poisoning 신호 전부를 포착하지 못한다.
  중간 글자 충돌, homoglyph 류(여기선 hex 라 무관), zero-value/dust 동반 여부 같은 보조 신호는 보지
  않는다. 이건 **warn** 신호(주의 환기)이지 deny 가 아니다 — 사용자가 확인한다.
- **counterparty-history 커버리지 의존.** `K` 가 불완전하면(히스토리 미동기화, 새 지갑) 진짜
  counterparty 를 모르므로 충돌도 못 잡는다 → false negative. 반대로 `K` 가 과대하면(예: 무관한 대량
  수령자 포함) 우연한 4글자 겹침으로 false positive 가 늘 수 있다.
- **임계값 트레이드오프.** `P`/`S` 는 false-positive ↔ false-negative 사이 튜닝. warn 정책이라
  보수적으로(낮은 임계값=더 많이 경고) 가는 편이 사용자 보호에 안전하나, 알림 피로를 유발할 수 있다.
- **chain scope.** 같은 EOA 주소가 여러 체인에서 등장 — `chain_id` 로 집합을 좁히면 정밀하지만,
  cross-chain 동일 주소 오염은 chain 별로 따로 봐야 한다.

## on-chain calls

**none (이 메서드의 핵심 경로는 온체인 read 가 아니다).**

- lookalike 비교 자체는 **순수 로컬 문자열 산술** — `eth_call`/account-state RPC 0회.
- 유일하게 I/O 가 끼는 지점은 **counterparty 집합 조달**이며, 그조차 온체인 view fn 호출이 아니다:
  host wallet store(로컬) 또는 off-chain explorer/indexer(account txlist). 후자를 택하면 그것은
  **off-chain data-API** 호출이지 contract view read 가 아니다. → contract/function/decoder/
  Multicall3 **해당 없음**.

## caching / ttl

- **lookalike 결과 cache key:** `(chain_id, candidate_lowercase, K_version)`. 단, `K`(counterparty
  집합)가 바뀌면 결과가 바뀌므로 집합 버전/해시를 key 에 포함해야 stale 충돌 판정을 피한다.
- **counterparty 집합 cache key:** `(chain_id, owner=$.root.from)` → `K`. 집합은 사용자 활동에 따라
  서서히 커진다 — 짧은 서명 세션 동안은 사실상 불변에 가깝다.
- **TTL:** 집합은 세션 단위 캐시(분 단위) 정도가 합리적; lookalike 산술은 입력이 같으면 결정적이라
  집합이 안 바뀌는 한 무기한 재사용 가능. 구체 TTL 수치는 본 repo 에 명시 상수가 없다 —
  **권장값, 출처 미확인**. indexer 로 집합을 재구성하는 설계면 indexer rate-limit 흡수를 위해 집합
  TTL 을 길게(분~시간) 잡는 게 유리하다.
- **위치:** local-first 설계면 `local-method-handlers.ts` 의 in-process 결과/`/v1/rpc` 서버 캐시.
  순수 산술 경로는 cache 가 없어도 cost 가 미미하다 — 비싼 건 집합 조달뿐.
- **budget:** `HARD_TIMEOUT_MS = 8000` (orchestrator, action 전체·batch 의 모든 planned call 공유)
  안에 들어야 한다. 집합이 캐시/동반 전달이면 산술만 — sub-ms. indexer cold-miss 면 단일 API 왕복이
  예산 내. 핸들러는 자체적으로 글로벌 예산보다 훨씬 짧은 deadline(예 ~2 s)을 두고, 자기 timeout 시
  batch 를 막지 말고 failure path(무 필드)로 빠져야 한다.

## failure & fallback (DORMANCY CONTRACT)

이 메서드의 카탈로그 caller(`transfer-address-poisoning`)는 `policy_rpc[].optional: true` +
`outputs[].required: false` (매니페스트 실측). 따라서 — **없는 사실이 verdict 를 뒤집어선 안 된다.**

- counterparty 집합 조회 실패/부재, malformed `candidate`, indexer 에러/timeout, 핸들러 self-timeout,
  param selector 미해소 → **결과 record 에서 `poisonCollision` 을 내보내지 않는다**(또는 `ok:false`
  로 result 를 통째로 비운다).
- host fold(`POLICY_RPC_METHODS.md` §1): missing/`ok:false` 결과 ⇒ `map[call_id]` 에 값 없음 ⇒
  `context.custom` 에 `poisonCollision` 없음 ⇒ 정책의 `context.custom has poisonCollision` guard 가
  **false** ⇒ `transfer-address-poisoning` 은 **INERT**(verdict 미생성) — false `warn` 도, false
  `pass` 도 없다.
- **절대 default 대입 금지.** 에러에 `poisonCollision: false` 를 박으면 실제 오염을 *조용히 통과*
  (false pass)시키고, `true` 를 박으면 무관한 전송을 *허위 경고*(false warn)한다 — 둘 다 금지. 부재가
  "판정 불가" 의 유일한 정직 신호다.
- `optional: true` 이므로 missing input 은 batch hard-fail 이 아니라 **pass 로 degrade**(이 정책만
  inert)한다. dormant/도달 불가 `/v1/rpc` dispatcher 는 구조적으로 안전하다.
- `lookalikeOf` 는 *성공* 호출 내에서도 독립적으로 optional: Tier 가 충돌을 찾아 `poisonCollision =
  true` 로 두되 `lookalikeOf` 를 생략해도 `transfer-address-poisoning`(이건 `poisonCollision` 만
  읽음)에 무영향.
- 요약: **실패 = 무 필드 = guard false = 정책 inert = 안전한 pass-through**. 이 메서드는 transfer
  warn enrichment 일 뿐 deny-closed venue 차단 로직이 아니다(HyperLiquid 경로와 무관).

## auth / cost / rate-limit

- **counterparty 집합을 host 가 동반 전달(local-first)하는 설계:** API key·네트워크 0. cost = 순수
  로컬 집합 비교(메모리)뿐. rate-limit 무관. **권장 1차 컷** — `address.similarity` 를
  `local-method-handlers.ts` 에서 처리하면 `/v1/rpc` POST 조차 안 한다.
- **서버가 indexer 로 집합을 재구성하는 설계:** Etherscan-style `account` txlist 호출 → `ETHERSCAN_API_KEY`
  (env) 필요 + rate-limit(free tier ~5 req/s — **출처 미확인**, 제공자/플랜 의존). owner 별 집합을
  길게 캐시해 steady-state 비용을 낮춘다. 모든 키는 서버 env 에서 오며 manifest/extension 에서 오지 않는다.
- lookalike 산술 자체는 어느 설계든 cost 무시 가능(글자 비교).

## activation

이 메서드를 구현(+ `schema/method-catalog.json` 등록)하면 다음 1개 카탈로그 정책이 dormant 에서 해제된다
(`POLICY_RPC_METHODS.md` §4 activation map 기준 `address.similarity` → 1 policy):

- **`transfer-address-poisoning`** (action `transfer`, `@severity("warn")`) —
  `context.custom.poisonCollision == true` 일 때 `Token::Action::"Erc20Transfer"` 를 `warn`.
  trigger = `action.tag == "erc20_transfer"`.

매니페스트는 이미 작성·컴파일되어 있고 정책은 dispatcher 가 `address.similarity` 를 서빙하기 전까지
dormant 상태다. 메서드를 `schema/method-catalog.json` 에 등록하는 것이 "implement" 의 일부다 — 등록
전까지 정책은 컴파일은 되지만 inert.

## primary-source references

- **소비자 매니페스트(wire shape 1차 출처):**
  `crates/policy-engine/tests/fixtures/policy_catalog_v2/action/transfer/transfer-address-poisoning/{manifest.json,policy.cedar}`
  — params(`chain_id`,`candidate`), projection(`$.result.poisonCollision → Bool`),
  `custom_context.fields={poisonCollision:Bool}`, `optional:true`, severity `warn`.
- **wire contract / projection / fold / dormancy:**
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §§1–2, §6(conventions: selectors,
  `optional:true`, local handlers).
- **local-first 처리 경로(순수 산술 메서드):**
  `browser-extension/backend/service-worker/local-method-handlers.ts` (`tryHandleLocally`) +
  `policy-rpc.ts` (`dispatchCallsV2`).
- **in-repo 부재 확인(verify against code, not this doc):** counterparty/wallet-history fetcher 는
  `crates/policy-server/sync/src/sources/fetchers/` 에 **없음**(`oracle / onchain / registry / venue
  / rpc / abi_decoder` 뿐). reuse 가능한 plumbing 0건 — 따라서 이 메서드의 집합 조달은 NET-NEW.
- **(서버-side 집합 재구성 택할 시) explorer txlist:** Etherscan API docs — https://docs.etherscan.io/
  (`account` module, `txlist` action). free-tier rate-limit 정확 수치는 **출처 미확인**(제공자/플랜 의존).
- **address-poisoning 공격 패턴(외부 1차/권고):** Etherscan address-poisoning advisory(지갑 히스토리
  vanity-주소 dust 스팸) + MetaMask 보안 권고. 구체 advisory URL·임계값(`P`/`S`=4)·dust 동반 통계는
  본 repo 코드에 상수로 박힌 바 없음 → **출처 미확인**(권고성 휴리스틱; 제품 튜닝 대상).
- **EIP-155**(chain id namespace) — https://eips.ethereum.org/EIPS/eip-155.
- **ERC-20 `transfer` / 20-byte address 표현** — https://eips.ethereum.org/EIPS/eip-20.
