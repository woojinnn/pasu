# method: portfolio.input_fraction_bps

status: existing (in method-catalog.json)

> Implementation spec for the future `/v1/rpc` server. This is **HOW to build the method**, not just
> its wire shape. Wire shape + projection are fixed by `schema/method-catalog.json` and by the one
> catalog manifest that calls it (`wallet/fraction-of-holdings/transfer-fraction-of-holdings/manifest.json`);
> do not change those — implement against them.

---

## purpose

한 transaction 이 지갑 보유분의 **몇 %** 를 쓰는지를 input asset 기준으로 계산한다. 절대 금액
(`amount`) 이나 USD 환산만으로는 "이게 내 잔고의 큰 비중인지" 를 판정할 수 없다 — 100 USDC 전송이
누구에겐 먼지, 누구에겐 전 재산이다. 이 method 는 `amount / balanceOf(owner)` 를 basis points
(1 bp = 0.01%) 로 환산해 **portfolio-relative magnitude** 라는 정적 디코드 불가능한 fact 를
제공한다. 이 fact 가 있어야 `transfer-fraction-of-holdings` 정책이 "잔고의 절반 초과를 옮기는
전송" (`holdingsBp > 5000`) 을 warn 으로 잡을 수 있다. Dambi 의 no-simulation 모델과 일관되게
이것은 transaction trace 가 아니라 **단일 잔고 read + 순수 산술** 이다.

## interface

고정 — `schema/method-catalog.json → methods["portfolio.input_fraction_bps"]` 그대로.

### params

| param | type | defaultSelector | required | 의미 |
|---|---|---|---|---|
| `chain_id` | `Long` | `$.root.chain_id` | yes | 잔고를 읽을 chain (EVM chain id, 예: 1) |
| `owner` | `String` | `$.root.from` | yes | 잔고 주인 = tx 서명자 (0x-주소) |
| `asset` | `AssetRef` | `$.action.inputToken.asset` | yes | 분모가 될 보유 자산 (아래 "asset 정규화") |
| `amount` | `String` | `$.action.inputToken.amount.value` | yes | 이 tx 가 쓰는 분자 raw amount (decimal string, base-unit) |

**호출 manifest 의 실제 selector** (catalog 의 transfer-shaped caller — swap-shaped default 와 다름):
```json
"params": {
  "chain_id": "$.root.chain_id",
  "owner":    "$.root.from",
  "asset":    "$.action.token",
  "amount":   "$.action.amount"
}
```
→ planner 는 두 selector 형태 (`$.action.inputToken.*` swap / `$.action.token`+`$.action.amount`
transfer) 를 모두 resolve 할 수 있어야 한다. 같은 method 가 caller 모양에 따라 다른 selector 로
호출된다.

### asset 정규화 (분모를 어디서 읽나)

`asset` 은 `TokenKey` (`crates/policy-server/asset-model/state/src/token/key.rs:20`,
`#[serde(tag="standard")]`) 로 도착한다. 분모 잔고 read 가 갈리는 지점:

- `{"standard":"erc20","chain":...,"address":"0x.."}` → ERC-20 `balanceOf(owner)`.
- `{"standard":"native","chain":...}` → native gas asset → `eth_getBalance(owner, latest)`
  (calldata 없는 RPC). **현재 호출 manifest 는 `erc20_transfer` 트리거뿐이라 erc20 경로만
  exercise 된다**; native 분기는 미래 native-transfer caller 를 위해 명세만 해두고, 미구현이면
  그냥 dormancy contract (아래) 로 field 를 drop 한다 — fabricated 0 으로 채우지 않는다.
- `erc721` / `erc1155` → fungible fraction 이 정의 불가 → field emit 안 함 (drop).

### result shape

```json
{ "bps": <Long> }     // 0 .. (unbounded; 분자가 잔고보다 크면 >10000 가능)
```
순수 scalar record. 추가 필드 (예: `numerator`, `denominator`) 를 디버그용으로 더 실어도 무방하나
**projection leaf 는 `bps` 하나** 다.

### projection (record → scalar leaf — 강제)

```
$.result.bps -> Long
```
v2 `materialize_v2` 는 record ProjectionType 을 받지 않는다 (`String|Long|Bool|Decimal|Set<String>`
만). 그래서 result record 는 manifest `outputs[].from = "$.result.bps"`, `type:"Long"`,
`field:"holdingsBp"` 로 **반드시 scalar leaf 로 투영**된다. `custom_context.fields.holdingsBp =
"Long"` 과 1:1 (ManifestV2::validate 강제). host fold 는 `ok:true` 시 unwrapped `$.result`
payload (즉 `{"bps":N}`) 를 그대로 `map[call_id]` 에 넣고, engine 이 `$.result.bps` 를 뽑는다.

## data source(s)

분모 한 번 read + 분자는 param 그대로 → **on-chain view read + 순수 산술**. 외부 가격 API/인덱서
불필요 (USD 환산이 아니라 same-asset ratio 이므로 decimals 도 약분되어 불필요).

- **ERC-20 분모 read → EXISTING-FETCHER-REUSABLE: `OnchainViewFetcher`**
  (`crates/policy-server/sync/src/sources/fetchers/onchain.rs`).
  - `OnchainCall::from_source(DataSource::OnchainView{ chain, contract=asset.address,
    function:"balanceOf(address)", decoder_id:"erc20_balance" }, encode_address(owner))`
    → selector `0x70a08231` + 32-byte owner 를 합성 (`decoder.rs:function_selector`/`encode_address`).
  - `fetch_one` (단건) 또는 `fetch_batch` (multicall3 `aggregate3`, allow_failure) 로 eth_call.
  - **decoder `"erc20_balance"` 이미 등록됨** (`decoder.rs:57` → `decode_u256_as_string`) →
    balance 를 decimal **string** 으로 반환 (u256 이 Long 범위를 넘으므로 string 으로 받아 산술은
    서버에서 U256/big-int 로). NET-NEW 디코더 불필요.
- **Native 분모 read → NET-NEW (소량)**: `eth_getBalance(owner,"latest")` 는 calldata 없는 RPC.
  `RpcRouter` 에 `eth_call` 은 있으나 `eth_getBalance` 헬퍼는 별도 추가 필요. 위 native 분기와
  함께 미구현 가능 (dormancy 로 drop).
- **bps 산술 → NET-NEW (사소)**: `floor(amount * 10000 / balance)`. 이 method 고유 로직 (산술
  reducer 는 어떤 fetcher 에도 없음). U256 big-int 로 곱한 뒤 나눠 overflow 회피.

## derivation algorithm

입력: `chain_id`, `owner`, `asset`(TokenKey), `amount`(decimal string).

1. **분자 파싱**: `numerator = U256::from_dec_str(amount)`. 파싱 실패 → field drop (dormancy).
2. **분모 read**:
   - erc20: `balance_str = OnchainViewFetcher.fetch_one(balanceOf(owner) @ asset.address @ chain)`
     → `denominator = U256::from_dec_str(balance_str)`.
   - native: `denominator = eth_getBalance(owner, latest)`.
   - 그 외 standard → drop.
   read 실패 (RPC error / revert / `!success`) → field drop (dormancy). **0 으로 대체 금지.**
3. **0-잔고 가드**: `denominator == 0`:
   - `numerator == 0` 이면 비율 미정의 → drop.
   - `numerator > 0` 이면 "0 잔고에서 양수 전송" — 산술적으로 ∞%. **추측 금지**: field 를 drop
     하거나 (권장: 정적 디코드만으로 unknowable), 명시 sentinel 없이 그냥 emit 안 함. caller 가
     없는 자산 전송은 어차피 체인에서 실패하므로 정책 신호로서의 가치 낮음 → **drop 권장**.
4. **bps 계산**: `bps = (numerator * 10000) / denominator` (U256 곱셈 후 나눗셈, **floor**).
   `numerator > denominator` 이면 `bps > 10000` (잔고 초과 전송 — 호출 정책은 `> 5000` warn 이라
   상한 클램프 불필요; 그대로 큰 값 emit 가능, 또는 가독성 위해 표현 상한만 둘 수 있음).
5. **Long 적합화**: `bps` 가 Long(i64) 범위를 넘으면 (이론상 numerator 가 천문학적일 때) — 호출
   정책 의미상 "엄청 큼" 이므로 i64::MAX 로 saturate 하거나 그대로 큰 Long 로 emit. 둘 다 verdict
   방향 (>5000 → warn) 을 안 뒤집으므로 안전. 단 **잔고보다 작은 정상 전송이 잘못 saturate 되지
   않도록** floor 산술이 정확해야 한다.
6. `{"bps": bps}` 반환.

**정직한 한계 (heuristic limits)**:
- **decimals 무시 가능 근거**: 분자·분모가 **같은 asset 의 같은 base-unit** 이므로 decimals 가
  약분된다 → decimals lookup 불필요·정확. (USD-cap 류와 달리 가격/소수 변환 없음.)
- **single read = single-asset 잔고만**: "portfolio" 라는 이름이지만 실제로는 **input asset 한
  종목** 의 잔고 대비 비율이다. cross-asset 총자산 대비 비중이 아니다. 호출 정책 (per-token
  transfer) 의미와는 정확히 일치하나, method 이름이 과대하게 들릴 수 있음 — verdict reason 도
  "이 토큰 잔고의 N%" 로 한정해 표현해야 한다.
- **시점 차이**: `balanceOf` 는 `latest` 블록 시점이고 tx 는 아직 미체결. mempool 의 다른 pending
  tx 가 잔고를 바꿀 수 있어 bps 는 **근사치** 다. 정책이 50% 같은 거친 임계만 보므로 허용 오차 내.
- **fee-on-transfer / rebasing 토큰**: `amount` 가 실제 차감액과 다를 수 있음 (이건 `token.metadata`
  method 의 영역). 이 method 는 명목 amount 기준 비율만 계산한다.

## on-chain calls

erc20 경로 (현재 호출되는 유일 경로):

- **chain**: `eip155:<chain_id>` — `chain_id` param 에서 (예: `1` → `eip155:1`). `RpcRouter` 가
  chain 별 provider 로 라우팅.
- **contract**: `asset.address` (ERC-20 토큰 컨트랙트).
- **view fn**: `balanceOf(address)` (selector `0x70a08231`), arg = `owner`.
- **multicall?**: 단일 call 이라 불필요하나, 같은 action 의 다른 enrichment (예: `oracle.usd_value`
  의 on-chain 분기) 와 **같은 chain 이면 `OnchainViewFetcher.fetch_batch` 의 multicall3
  `aggregate3` 로 묶어 RTT 1 회로 합치는 것을 권장** (allow_failure=true 이므로 한 leg 실패가 다른
  leg 를 죽이지 않음 — 실패 leg 는 그 field 만 drop).

native 경로: `eth_getBalance(owner,"latest")` (calldata 없음, multicall 불가 — 별도 RPC).

## caching / ttl

- **cache key**: `(chain_id, owner, asset_key)` — `amount` 는 키에서 **제외** (분모만 캐시; 분자는
  순수 산술이라 매번 재계산). asset_key = `balanceOf` 결과를 캐시하는 단위.
- **value**: 분모 balance (decimal string).
- **ttl**: 짧게 — **2~5 s** 권장. 잔고는 빠르게 변하고, 한 사용자의 연속 액션 (approve→transfer
  등) 내에서만 재사용하면 충분. per-action 단위로만 의미 있으므로 **per-action results map**
  (call_id 반복 시 fresh map) 컨벤션과 충돌 없음.
- **where**: `/v1/rpc` 서버 in-process LRU (host 측). 익스텐션 `dispatchCalls` 는 캐시하지 않음.
- **budget**: orchestrator `HARD_TIMEOUT_MS = 8000`. 단일 `eth_call` (또는 batch leg) 1 RTT
  (~100–500 ms public RPC) → 여유. 캐시 hit 시 0 RTT. timeout 초과 시 host 가 `ok:false` 반환 →
  field drop (dormancy). **느린 RPC 가 batch 전체를 8 s 까지 끌지 않도록 per-call 타임아웃**
  (예: 2–3 s) 을 두고 초과 시 그 leg 만 drop.

## failure & fallback (DORMANCY CONTRACT)

이 method 는 catalog 에서 **`optional: true` + `outputs[].required: false`** 로 호출된다
(manifest 확인됨). 따라서 fail-safe 방향이 강제된다:

- 어떤 실패 (RPC error, revert, `!success`, amount 파싱 실패, denominator==0, native 미구현,
  non-fungible asset, per-call timeout) 든 → **`bps` field 를 emit 하지 않는다**.
- host fold: 해당 call 이 `ok:false` 또는 result 에 `bps` 없음 → `map[call_id]` 에 `holdingsBp`
  가 안 들어감.
- 결과: `context.custom` 에 `holdingsBp` 부재 → 정책의 `context.custom has holdingsBp` 가 **false**
  → `forbid` when-clause 전체가 false → **정책 INERT** (warn 도 pass 도 강제 안 함; 다른 정책의
  verdict 에 무영향).
- `optional:true` 이므로 이 한 call 의 실패가 batch 전체를 hard fail 시키지 않는다 → 그 action 은
  다른 정책 결과대로 자연 진행 (degrade to pass for *this* policy, never a spurious block).
- **절대 금지**: 실패를 `bps:0` 이나 `bps:10000` 같은 **default 로 메우는 것**. 0 은 정책을
  silent-pass 로, 큰 값은 spurious-warn 으로 **verdict 를 뒤집는다**. 모르면 비운다 — 추측을
  fact 로 둔갑시키지 않는다. (no-simulation 모델의 정직성 계약.)

## auth / cost / rate-limit

- **API keys (env)**: 외부 price/indexer API 불필요. 필요한 것은 chain RPC endpoint 뿐 —
  `RpcRouter` 의 provider 설정 (public RPC 또는 `*_RPC_URL` env, 서버 RPC config 소관). 별도
  per-method 키 없음.
- **per-call cost**: eth_call 1 회 (캐시 miss 시). 무료 public RPC 또는 저비용 archive-불요
  `latest` read. native 는 `eth_getBalance` 1 회.
- **rate-limit**: public RPC 의 RPS 제한이 유일 제약. 같은 (chain,owner,asset) 의 반복 액션은
  TTL 캐시가 흡수; 같은 action 내 동일 chain enrichment 는 multicall3 로 1 RTT 합산. burst 시
  per-call timeout 으로 8 s budget 보호.
- **캐시가 흡수하는 방식**: 분모 캐시 key 에 amount 를 빼서, 사용자가 슬라이더로 금액만 바꿔가며
  재서명하는 흔한 UX 에서도 RPC 재호출 0 회 (분자 산술만 재실행).

## activation

이 method 를 `/v1/rpc` 가 서빙하면 dormant 해제되는 catalog policy:

- **`transfer-fraction-of-holdings`** (`wallet/fraction-of-holdings/transfer-fraction-of-holdings`,
  catalog D6) — `forbid` on `Token::Action::"Erc20Transfer"` when `context.custom.holdingsBp > 5000`,
  `@severity("warn")`. 잔고 절반 초과 전송을 서명 전 warn.

(activation map: `POLICY_RPC_METHODS.md` §4 — 이 method = 1 policy.)

## primary-source references

- **ERC-20 `balanceOf(address)`** — EIP-20, "Methods" section, `balanceOf` (분모 read 의 표준
  함수 시그니처). https://eips.ethereum.org/EIPS/eip-20
- **`eth_call` / `eth_getBalance`** — Ethereum JSON-RPC API spec (`eth_call`,
  `eth_getBalance` with `"latest"` block tag, native 분모 경로).
  https://ethereum.org/en/developers/docs/apis/json-rpc/
- **Multicall3 `aggregate3`** — 같은-chain enrichment 묶음용 (allow_failure per-call). 컨트랙트
  사양은 코드 내 `crates/policy-server/sync/src/sources/fetchers/rpc/multicall.rs` 가 SSOT.
  외부 표준 컨트랙트 출처: https://github.com/mds1/multicall (1차 GitHub spec).
- **basis point 정의** (1 bp = 0.01% = 1/10000) — 일반 금융 단위, 표준 RFC/EIP 없음 → **출처
  미확인** (산술 정의는 본 spec §derivation 이 SSOT).
- **호출처/투영 SSOT (repo-internal, 1차)**: `schema/method-catalog.json`
  (`methods["portfolio.input_fraction_bps"]`) +
  `crates/policy-engine/tests/fixtures/policy_catalog_v2/wallet/fraction-of-holdings/transfer-fraction-of-holdings/manifest.json`
  + `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` (wire contract §1, projection §2).
