# ActionBody 확장 가이드 — 두 축 (domain / sub-action) recipe

> `ActionBody` 를 확장하는 정규 절차. 1차 출처 = repo 코드 실측. 도메인 수는 늘어난다 — 작성 전 `grep -n "pub enum ActionBody" crates/policy-server/asset-model/action/src/lib.rs` 와 대상 `action/<domain>/mod.rs` 를 직접 재확인.
> 참조 심볼은 `file 의 symbol` 형식 (line 번호는 갱신 시 stale 되므로 보조). 갱신 시 `grep` 으로 재확인.
> 관련: 온보딩 방법론 spine = `PROTOCOL_ONBOARDING_AND_TESTING.md`(같은 디렉토리; 특히 §4d live_field enrichment 가 본 가이드 §2.5 를 cross-ref). (확장 제안 `SCHEMA_EXTENSION_PROPOSALS.md` · 통합 playbook `TIER_AB_PLAYBOOK.md` 는 gitignored `docs/` 에 있어 fresh clone 엔 없음 — optional.)

## 0. 확장 축은 둘

| 축 | 무엇 | 진입점 | 빈도 |
|---|---|---|---|
| **축 1 — 새 domain** | `ActionBody` enum 에 variant | `action/mod.rs` 의 `ActionBody` (`#[serde(tag="domain")]`) | 드묾·큼 |
| **축 2 — 새 sub-action** | 기존 `<Domain>Action` enum 에 variant | `action/<domain>/mod.rs` 의 `<Domain>Action` (`#[serde(tag="action")]`) | 흔함 |

직렬화 형태는 **이중 internally-tagged flat**: `ActionBody::Perp(PerpAction::OpenPosition(..))` → `{"domain":"perp","action":"open_position", ...payload}`. manifest 의 `emit.body` 는 가독성 위해 **nested-twice** (`{domain:"perp", perp:{action:"open_position", open_position:{...}}}`) 로 쓰고, `action_builder.rs` 의 `flatten_body` 가 flat 으로 normalize 한다.

## 0.1 ActionBody → lowering_v2 → cedarschema 계약

Tier 3 확장은 `ActionBody` Rust 타입만 추가하면 끝이 아니다. `ActionBody` 는 **디코더/시뮬레이터가 이해하는 normalized intent schema** 이고, 정책 엔진은 그 타입을 직접 읽지 않는다. 정책 엔진은 `lowering_v2` 가 만든 Cedar request context 를 읽고, 그 context shape 는 `schema/policy-schema/actions/**/*.cedarschema` 가 정의한다.

```text
raw tx / typed-data
  → manifest/Tier1 + builder/Tier2
  → ActionBody                         crates/policy-server/asset-model/action/src/**
  → lowering_v2::lower_action          crates/policy-engine/src/lowering_v2/**
  → LoweredAction {
       principal: Wallet::"<tx.from>",
       action_uid: <Namespace>::Action::"<PascalAction>",
       resource: Protocol::"<tx.to>",
       context: { ... }                must conform to .cedarschema
     }
  → Cedar policy engine
```

따라서 새 action/field 를 추가할 때 책임은 셋으로 분리된다:

| 레이어 | 위치 | 책임 |
|---|---|---|
| **ActionBody schema** | `crates/policy-server/asset-model/action/src/<domain>/**` | tx 의미를 protocol-agnostic intent 로 표현. Rust/serde/tsify source-of-truth. |
| **lowering** | `crates/policy-engine/src/lowering_v2/<domain>/<action>.rs` | `ActionBody` payload 를 Cedar context JSON 으로 변환. camelCase, U256 hex, optional omit, `LiveField<T>.value` flatten. |
| **cedarschema** | `schema/policy-schema/actions/<domain>/<action>.cedarschema` | Cedar 가 정책에서 읽을 action/context 타입 선언. `action "<Pascal>" appliesTo { principal: Wallet, resource: Protocol, context: <Context> }`. |

예: `AmmAction::Swap(SwapAction)` 은 `action/amm/swap.rs` 에 Rust schema 가 있고, `lowering_v2/amm/swap.rs` 가 `Amm::Action::"Swap"` + `Amm::SwapContext` JSON 을 만들며, `schema/policy-schema/actions/amm/swap.cedarschema` 가 `SwapContext` 필드를 선언한다. 셋 중 하나라도 필드명/타입/필수성/uid 가 어긋나면 policy strict validation 이 실패하거나, 더 위험하게 policy 작성자가 필요한 필드를 못 본다.

lowering 규칙:
- Rust `snake_case` → Cedar `camelCase` 를 손으로 매핑한다. `serde_json::to_value` 로 blind 변환 금지.
- `U256`/`U128` 는 lower-hex string(`"0x..."`), `Address` 는 lowercase `0x...`.
- `Long` 은 JSON number 로 emit 한다.
- Rust enum/union 은 Cedar 에서 `{ kind: "...", ... }` 또는 `{ name: "...", ... }` discriminated record 로 표현한다.
- `LiveField<T>` 는 `.value` 만 노출한다. source/ttl/synced_at metadata 는 policy context 에 노출하지 않는다.
- optional 은 `null` 로 emit 하지 말고 absent 면 생략한다.
- every action context 는 `meta: Core::ActionMeta` 를 포함한다.
- `ctx.lowered(r#"<Namespace>::Action::"<PascalAction>""#, Value::Object(context))` 의 action uid 는 cedarschema 의 `action "<PascalAction>"` 와 정확히 맞아야 한다.

cedarschema 규칙:
- `core.cedarschema` 는 `Wallet`, `Protocol`, `Core::TokenRef`, `Core::ActionMeta`, `Amm::AmmVenue` 같은 shared type 을 제공한다.
- action file 은 자기 namespace 안에 `<Action>Context`, `<Action>CustomContext = {};`, `action "<PascalAction>" appliesTo ...` 를 정의한다.
- `custom?: <Action>CustomContext` 는 manifest `custom_context` 주입 슬롯이다. base field 와 custom field 충돌은 `compose_per_policy` 가 거부한다.
- Cedar 4.10 은 enum/union/generic 이 없으므로, Rust 타입을 그대로 옮기는 게 아니라 Cedar-compatible projection 을 설계해야 한다.

conformance rule: 새/변경 action 은 leaf lowering test 에서 `lower_action` 결과의 `context` 를 실제 `compose_per_policy` schema 로 strict validate 해야 한다. 이 테스트가 ActionBody ↔ lowering ↔ cedarschema drift 를 잡는 최종 안전망이다.

## 1. Compile-coupling map (확장 비용의 본질)

확장 touchpoint 는 세 부류다:

**(a) 컴파일러가 강제 (✅ 빠뜨리면 빌드 실패 — 안전)** — Rust exhaustive `match` 5곳, wildcard 없음:

| match | 위치 (symbol) | 깨지는 축 |
|---|---|---|
| `impl Reducer for ActionBody` | `crates/policy-server/asset-model/transition/src/apply.rs` | 축 1 (domain) |
| `lower_action` | `policy-engine/src/lowering_v2/dispatch.rs` | 축 1 (domain) |
| `impl Reducer for <Domain>Action` | `crates/policy-server/asset-model/transition/src/effect/<domain>.rs` 또는 `effect/<domain>/mod.rs` | 축 2 (sub-action) |
| `<Domain>Action::action_tag()` | `crates/policy-server/asset-model/action/src/<domain>/mod.rs` | 축 2 (sub-action) |
| `<domain>::lower` dispatch | `policy-engine/src/lowering_v2/<domain>/mod.rs` | 축 2 (sub-action) |

**(b) 사람이 챙겨야 함 (⚠️ 컴파일은 통과 — silent gap, sub-action 1개당 Cedar 등록 3 site)**:

새 `.cedarschema` 를 만든 뒤 **세 곳 모두** 등록해야 lowering schema 가 합성된다 (auto-discovery 아님). 하나라도 빠지면 컴파일은 통과하나 런타임/테스트에서 실패:

1. **`policy-engine/src/schema/mod.rs`** — `const <NAME>_SCHEMA = include_str!(...)` + `SHIPPED_SCHEMA_FILES` 배열 (통합 schema 합성용).
2. **`policy-engine/src/schema/action_name.rs`** — `REGISTERED_ACTIONS` 배열에 snake_case action tag 추가 (+ 그 `len()` assertion 갱신).
3. **`policy-engine/src/schema/per_policy.rs`** — `RESOLVER_TABLE` 에 `ActionEntry { domain, action_tag, schema_text: <NAME>_SCHEMA, pascal_stub: "<PascalAction>" }` 추가 + import 에 `<NAME>_SCHEMA` 추가 (+ 그 `len()` assertion 갱신). **이게 `compose_per_policy` 의 (domain, action_tag)→schema 권위 테이블** — 누락하면 conformance 가 `MissingAction(<NS>::Action::"<Pascal>")` 으로 잡는다(안전망 작동, 2026-05-31 Morpho `SetAuthorization` 에서 실측).

> ⚠️ **REGISTERED_ACTIONS(2) 와 RESOLVER_TABLE(3) 은 비대칭이다 — action tag 가 domain 간 충돌하면.** `REGISTERED_ACTIONS` 는 **dedup 된 tag 집합**(`registered_actions_unique` 테스트가 중복 거부)이고, `RESOLVER_TABLE` 은 **(domain, action_tag) 키**(같은 tag 라도 domain 마다 별도 행)다. 따라서 새 sub-action 의 tag 가 **다른 domain 에 이미 있으면**(예: `staking::stake` 추가 시 `liquid_staking` 가 이미 `"stake"` 등록): RESOLVER_TABLE 엔 새 행 추가(그 domain 의 schema 로 분기), **REGISTERED_ACTIONS 엔 추가 안 함**(이미 있음 — 추가하면 `duplicate action` 패닉). len assertion 도 RESOLVER 는 +1, REGISTERED 는 +0. (2026-06-03 Aave safety-module `staking::stake` 가 `liquid_staking::stake` 와 충돌 → conformance `registered_actions_unique` 가 잡음. 안전망 작동.)

또한 **compile-forced (위 5곳 외 추가)**: `policy-server/sync/src/actions/walk/<domain>.rs` 의 walk + apply match 두 곳도 exhaustive — live_inputs 없는 action 은 `<DomainAction>::<New>(_) => {}` arm 추가 (DelegateBorrow 선례).

**(b′) live_field 전용 touchpoint (⚠️ silent — catch-all 이 삼킴)**: action 에 `LiveField` 를 **추가**할 때(=§2.5 enrichment)는 세 곳이 더 있는데 **컴파일러가 안 잡는다**:
- `crates/policy-server/sync/src/live/walker.rs` 의 `ActionSlot` enum — variant 추가(enum 이라 누락해도 컴파일 통과).
- `crates/policy-server/sync/src/actions/args.rs` 의 `resolve_args` — `_ => Vec::new()` **catch-all** 이 있어 arm 누락 시 빈 args 로 조용히 진행(view 인자 미전달).
- `mappers/.../action_builder.rs` 의 `live_input_default` — `_ => JsonValue::Null` **catch-all** → skeleton 누락 시 decode 가 `null` 거부로 실패(loud) 하거나 Option 이면 통과(silent).
이 셋은 (c) conformance/decode 테스트 또는 §4d golden 으로만 잡힌다. 상세 = `PROTOCOL_ONBOARDING_AND_TESTING.md §4d` 의 5-touchpoint 표.

**(c) 안전망**: `lowering_v2/<domain>/<sub>.rs` 의 leaf test 가 `super::super::test_support::assert_conforms(tag, body, meta)` 호출 → lowering 산출 `context` 를 **실제 Cedar schema 로 strict 구성**. `test_support`(sample builder + `assert_conforms`)는 **각 domain 의 `lowering_v2/<domain>/mod.rs` 에 자체 정의** (token/perp/lending/amm/launchpad/airdrop 6곳, sample helper 는 domain 마다 다름). Rust struct ↔ Cedar context 의 rename·타입·누락·과다를 패닉으로 잡는다. (b) 의 silent gap 을 leaf test 하나로 막는 장치이므로 **반드시 추가**.

**(d) generic — 확장 시 변경 0**:
- `action_builder.rs` 의 `flatten_body`: `body["domain"]` → `body[domain]["action"]` → payload 를 구조적으로 추출 (domain/action 이름 hardcode 없음).
- `registryV2/scripts/build-index.ts`: `emit.body.domain` allow-list 검증 없음.
- TS `policy_engine_wasm.d.ts`: `tsify` 가 `scripts/wasm-build.sh` 의 `wasm-pack` 단계에서 자동 생성. 손으로 안 건드림.

## 2. 축 2 — 기존 domain 에 새 sub-action 추가

`PerpAction` 에 `twap_order` 를 넣는 예. 의존 순서 ①→⑦:

**① struct** · `action/perp/twap.rs` (신규)
```rust
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
// + 필요한 primitive (PerpVenue, MarketRef, LiveField, ...)

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TwapOrderAction {
    pub venue: PerpVenue,
    pub market: MarketRef,
    // 정적 = plain 타입. runtime 조회값 = LiveField<T> 로 감싼다.
    pub live_inputs: TwapOrderLiveInputs,
}
// U256/Address 등은 #[tsify(type = "string")] 로 JS 표현 명시 (erc20_approve.rs 참고)
```

**② enum 등록** · `action/perp/mod.rs`
```rust
pub mod twap;                 // 추가
pub use self::twap::*;        // 추가

pub enum PerpAction {
    // ...
    TwapOrder(TwapOrderAction),     // 추가
}

impl PerpAction {
    pub const fn action_tag(&self) -> &'static str {
        match self {
            // ...
            Self::TwapOrder(_) => "twap_order",   // ✅ 안 넣으면 컴파일 에러
        }
    }
    // venue 도메인은 venue_name() match arm 도 (perp/amm/lending). token 은 None 고정.
}
```

**③ effect** · `effect/perp/mod.rs` + `effect/perp/twap.rs`
```rust
// effect/perp/mod.rs 의 impl Reducer for PerpAction
Self::TwapOrder(a) => a.apply(state, ctx),   // ✅ 강제

// effect/perp/twap.rs (신규): state transition 계산
impl Reducer for TwapOrderAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> { ... }
}
```
> effect 측 구조는 domain 마다 다르다 — `token.rs`/`airdrop.rs`/`launchpad.rs` 는 단일 파일, `amm`/`lending`/`perp` 는 디렉토리. sub-action 이 적으면 단일 파일에 `impl` 추가, 많으면 leaf 파일 분리.

**④ lowering** · `lowering_v2/perp/twap.rs` + `lowering_v2/perp/mod.rs`
```rust
// lowering_v2/perp/twap.rs (신규)
pub(crate) fn lower(a: &TwapOrderAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let context = serde_json::json!({ /* Cedar TwapOrderContext 필드와 1:1 */ });
    Ok(ctx.lowered("Perp::Action::\"TwapOrder\"", context))   // action_uid 규약은 기존 leaf 참고
}

// lowering_v2/perp/mod.rs 의 dispatch match
PerpAction::TwapOrder(a) => twap::lower(a, ctx),   // ✅ 강제
```
> lowering_v2 는 전 domain 디렉토리 구조 (per-action leaf 파일).

**⑤ Cedar schema** · `schema/policy-schema/actions/perp/twap_order.cedarschema` (신규) + loader 등록
```cedar
namespace Perp {
    type TwapOrderContext = { /* ③④ 의 struct/lowering 과 동일 필드 */ };
    action "TwapOrder" appliesTo {
        principal: Wallet, resource: Protocol, context: TwapOrderContext
    };
}
```
```rust
// policy-engine/src/schema/mod.rs  ⚠️ 수동 — 빠뜨리기 쉬움
const PERP_TWAP_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/twap_order.cedarschema");
// + SHIPPED_SCHEMA_FILES 배열에 PERP_TWAP_ORDER_SCHEMA 추가
```

**⑥ conformance test** · `lowering_v2/perp/twap.rs` 의 `#[cfg(test)] mod tests`
```rust
#[cfg(test)]
mod tests {
    use super::super::test_support::{assert_conforms, sample_market, sample_venue /* ... */};
    #[test]
    fn conforms() {
        let body = ActionBody::Perp(PerpAction::TwapOrder(/* sample */));
        assert_conforms("twap_order", &body, &meta);   // perp/mod.rs 의 test_support (그 domain 자신의 것)
    }
}
```
`assert_conforms` + sample helper 는 그 domain 의 `<domain>/mod.rs` 의 `test_support` 에서 import. ⑤ 의 schema 와 ③④ 의 Rust 필드가 어긋나면 여기서 즉시 패닉 — (b) silent gap 의 안전망.

**⑦ manifest** · `registryV2/manifests/<protocol>/.../twap@1.0.0.json`
```jsonc
"emit": { "strategy": "single_emit", "body": {
    "domain": "perp",
    "perp": { "action": "twap_order", "twap_order": { /* $args.* placeholder */ } }
}}
```

자동 (변경 0): TS `.d.ts`, `build-index.ts`, `flatten_body`.

## 2.5 — 기존 action 에 live_field 추가 (enrichment, 축 무관)

축 1/2 가 **새 action/domain** 을 만드는 거라면, 이건 **이미 있는 action** 에 host-populated `LiveField<T>` 를 하나 더 다는 것이다. 디코드된 필드가 추상 단위(shares/내부 index/wrapped 수량/rate)라 사용자에게 안 읽힐 때 환산값을 보여주려고 한다. **왜·언제 하는지 = `PROTOCOL_ONBOARDING_AND_TESTING.md §4d`(enrichment decision-tree + self-check). 여기는 어떻게.** 정본 미러 = `lending::supply`(`SupplyLiveInputs`).

> ⚠️ **아래 스니펫은 `T = U256` 사례다.** `T` 가 `Decimal`/`bool`/struct/tuple 이면 ④ skeleton·② apply coercion·⑤ lowering·cedar type 이 다르다 — `§4d 의 live_field 타입별 매핑표` 참조(`SupplyLiveInputs` 가 U256/Decimal/bool/struct 변종 전부 보유).

핵심 갈림: live_field 의 source view 가 **calldata 인자를 쓰나?**
- 인자 없는 view / oracle / derived → manifest `live_inputs.source` 만(⑤). Rust 0.
- **calldata 인자 필요한 view** (`getPooledEthByShares(shares)`) → `DataSource::OnchainView` 에 args 필드 없음 → 아래 ②③ Tier 2 필수.

**① reducer — LiveInputs struct** · `action/<domain>/<action>.rs`
```rust
use policy_state::LiveField;   // 추가
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WrapLiveInputs { pub expected_wsteth: LiveField<U256> }   // non-optional
// WrapAction 에: pub live_inputs: WrapLiveInputs   (serde default 없음 — manifest 가 항상 emit)
```
> `pub use self::<action>::*` wildcard 재export 면 신규 `*LiveInputs` 자동 노출. mod.rs 의 "no LiveInputs" 주석 갱신.

**② sync ActionSlot + walk/apply** · `crates/policy-server/sync/src/live/walker.rs` + `sync/src/actions/walk/<domain>.rs`
```rust
// walker.rs ActionSlot enum 끝에:  (⚠️ silent — enum)
LiquidStakingWrapExpectedWsteth,
// actions/walk/<domain>.rs:  walk → push_if_stale, apply → set_field (lending.rs 미러)
fn walk_wrap(w:&WrapAction, ix:usize, now:Time, st:&mut Vec<StaleField>, sx:&mut WalkStats) {
    push_if_stale(st, sx, &w.live_inputs.expected_wsteth, now, ix, ActionSlot::LiquidStakingWrapExpectedWsteth);
}
fn apply_wrap(w:&mut WrapAction, slot:&ActionSlot, value:Value, now:Time) {
    if matches!(slot, ActionSlot::LiquidStakingWrapExpectedWsteth) {
        if let Some(v) = value_to_u256(&value) { set_field(&mut w.live_inputs.expected_wsteth, v, now); }
    }
}
// + actions/walk/mod.rs 의 walk_body/apply_value_to_action 에서 그 domain arm 이 위 walk/apply 호출 (없으면 pub mod + dispatch arm 추가)
```

**③ args — calldata 인자 추출** · `crates/policy-server/sync/src/actions/args.rs` (인자 있는 view 만)
```rust
use policy_transition::action::liquid_staking::LiquidStakingAction;
use crate::fetchers::decoder::encode_u256;   // encode_address/encode_u256 둘 다 이미 존재
// resolve_args match 에:
ActionSlot::LiquidStakingTransferSharesPooledEth => {
    if let ActionBody::LiquidStaking(LiquidStakingAction::TransferShares(t)) = &action.body {
        return encode_u256(t.shares).to_vec();   // shares 를 getPooledEthByShares 인자로
    }
    Vec::new()
}
```

**④ generic 엔진 — skeleton** · `mappers/.../action_builder.rs` `live_input_default`
```rust
(Some("liquid_staking"), Some("wrap"), "expected_wsteth") => JsonValue::String("0".into()),  // U256 skeleton
// layout 은 default Nested → 보통 live_input_layout 변경 0
```

**⑤ Cedar field + lowering** · `<action>.cedarschema` + `lowering_v2/<domain>/<action>.rs`
```cedar
// cedarschema: LiveField<T> → inner T flatten, non-optional, source meta 비노출
expectedWsteth: String,   // U256 hex, host-populated
```
```rust
// lowering: .value 추출
m.insert("expectedWsteth".into(), Value::String(u256_hex(action.live_inputs.expected_wsteth.value)));
// + test_support 에 skeleton 헬퍼: fn live_u256()->LiveField<U256>{ LiveField::new(U256::ZERO, oracle_src(), now()) }
// + conform 테스트 생성자에 live_inputs: WrapLiveInputs{ expected_wsteth: live_u256() } 추가 (안 하면 assert_conforms 패닉)
```

**⑥ manifest source** · `registryV2/manifests/<p>/.../<func>@1.0.0.json`
```jsonc
"live_inputs": { "expected_wsteth": { "source": {
  "kind":"onchain_view", "chain":"$chain", "contract":"$to",
  "function":"getWstETHByStETH(uint256)", "decoder_id":"lido_wsteth_by_steth" }, "ttl_s": 30 } }
```

**검증**: `cargo test -p policy-transition -p policy-engine`(serde + conformance) → `npm run check:manifest`(emit.body shape) → golden(§4d: `source.function` pin, 값 아님) → `cargo test --workspace`. ②③④ 는 catch-all 이 삼키는 silent touchpoint(§1 b′)라 누락 시 decode-error/conformance 로만 드러난다.

## 3. 축 1 — 새 domain 추가

`OffchainExchange` 같은 새 off-chain venue domain 을 추가하는 가상 예. **각 sub-action 마다 축 2 의 ①~⑦ 를 반복** + 최상위 3곳:

**⓪ domain enum 신규** · `action/offchain_exchange/mod.rs`
```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum OffchainExchangeAction { /* sub-action variants */ }
// + sub-action struct 파일들 (축 2 ①②)
```

**Ⓐ ActionBody 등록** · `action/mod.rs`
```rust
pub mod offchain_exchange;                              // 추가
pub use offchain_exchange::OffchainExchangeAction;      // 추가

pub enum ActionBody {
    // ...
    OffchainExchange(OffchainExchangeAction),   // serde 가 "offchain_exchange" 자동 매핑
}
```

**Ⓑ 최상위 effect dispatch** · `apply.rs` + `effect/`
```rust
// apply.rs 의 impl Reducer for ActionBody
Self::OffchainExchange(a) => a.apply(state, ctx),       // ✅ 강제
// effect/offchain_exchange.rs (단일) 또는 effect/offchain_exchange/ (디렉토리) 신규
// + effect/mod.rs 에 pub mod offchain_exchange;
```

**Ⓒ 최상위 lowering dispatch** · `lowering_v2/dispatch.rs` + `lowering_v2/`
```rust
// dispatch.rs 의 lower_action
ActionBody::OffchainExchange(a) => super::offchain_exchange::lower(a, &ctx),   // ✅ 강제
// lowering_v2/offchain_exchange/mod.rs 신규 + lowering_v2/mod.rs 에 mod
```

**Ⓒ′ (구버전 — 더 이상 불필요, 2026-06-03 정정)**: 과거엔 `oracle.rs` 의 `VALID_DOMAINS` 배열에 새 domain serde tag 를 등록해야 했다(Lido `liquid_staking` 실측). 그러나 **`VALID_DOMAINS` 는 cross-crate drift trap 이라 제거됨** — `oracle.rs` 가 이제 L3 domain-validity 를 hand-list 없이 **L2 typed round-trip**(`#[serde(tag="domain")]` 디시리얼라이즈 성공 여부)으로 검증한다(oracle.rs L137-145: "The former `VALID_DOMAINS` array was a cross-crate drift trap ... dropped"). 따라서 새 domain 은 **하니스 등록 불필요** — `ActionBody` enum 에 variant 추가하면 serde 가 자동 인식. (Aave dogfood 가 이 stale 지시를 확정.)

**Ⓓ 그다음** 각 sub-action 마다 축 2 의 ③~⑦ (effect leaf / lowering leaf / cedarschema + loader 등록 / conformance / manifest). 새 domain 은 `lowering_v2/<domain>/mod.rs` 에 자체 `test_support` 모듈(sample builder + `assert_conforms`)을 신설한다 (token/perp 패턴 복제).

**Cedar namespace**: 새 `namespace OffchainExchange { ... }` 블록을 action 파일들에 선언. `schema/mod.rs` 의 `merge_namespace_blocks` 가 namespace 단위로 병합하므로 별도 core 변경 불필요 (단 ⑤ 의 loader 등록은 action 파일별 필수).

**meta / sig-routing 재사용**: domain 추가가 off-chain 서명 기반이면 `ActionMeta.nature = OffchainSig` (`action/mod.rs` 의 `ActionNature`) 를 그대로 쓰고, A.1 에서 일반화한 `sig-routing.ts` 의 typed_data → manifest lookup 이 그대로 처리한다. **새 routing 인프라 불필요** — domain 추가는 ActionBody 표현만 늘리는 것.

**(선택) TS**: `orchestrator.ts`/`multicall-handler.ts` 가 domain 문자열로 분기하는 곳은 현재 `"multicall"`/`"unknown"` 뿐. 새 domain 이 그런 특수 처리를 요구할 때만 추가, faithful ActionBody 면 불필요.

## 4. 대비표 — 축 2 대비 축 1 의 추가분

| 단계 | 축 2 (sub-action) | 축 1 (domain) 추가분 |
|---|---|---|
| `action/mod.rs` ActionBody enum | — | variant + `pub mod`/`pub use` |
| `apply.rs` 최상위 Reducer match | — | arm ✅ |
| `lowering_v2/dispatch.rs` 최상위 match | — | arm ✅ |
| `action/<d>/mod.rs` | variant + `action_tag()` arm ✅ | 디렉토리·enum 신규 |
| effect | match arm + leaf ✅ | 디렉토리/파일 신규 + `effect/mod.rs` |
| lowering | leaf + mod arm ✅ | 디렉토리 신규 + `lowering_v2/mod.rs` |
| Cedar `.cedarschema` + `schema/mod.rs` 등록 | 1 파일 + 수동 ⚠️ | namespace + N 파일 + 수동 ⚠️ |
| conformance `assert_conforms` | leaf test | 동일 |
| manifest `emit.body` | ✓ | ✓ |
| TS `.d.ts` / build-index / flatten | 🤖 자동 | 🤖 자동 |

## 5. Decision rule — 언제 무엇을

1. **기존 domain(`action/mod.rs` 에서 현재 목록 직접 확인) 중 하나의 의미에 맞음 → sub-action.** (예: 신규 perp 주문 타입 → `PerpAction` variant.)
2. **어디에도 안 맞고 구조화 가치 > 비용 → 새 domain.** Cedar policy 작성자에게 새 namespace 를 노출하는 큰 결정이므로 표현 이득이 분명할 때만.
3. **가치 < 비용 또는 1차 출처 불충분 → `Unknown` + metadata 유지.** opaque/admin/출처 불충분 호출을 억지 domain 으로 포장하지 않는다. scope analyzer 로서 과장하지 않는 것이 정직하다. 단, 나중에 구조화 가치가 생겨 domain/action 이 추가되면 기존 `Unknown` corpus/manifest 는 새 action 으로 마이그레이션한다.

> deferred 후보 (`SCHEMA_EXTENSION_PROPOSALS.md` — gitignored `docs/`, fresh clone 부재; §0 참조): 새 off-chain exchange domain, venue-specific live_input catalog 등. 둘 다 위 rule 2 의 "구조화 가치 > 비용" 재평가 후 진입.

## 6. 검증

```bash
cargo test -p policy-transition   # action serde round-trip + effect Reducer
cargo test -p policy-engine        # lowering + conformance gate (assert_conforms)
./scripts/wasm-build.sh            # tsify .d.ts regenerate (Rust 변경 시)
```

축 2 최소 게이트 = `cargo test -p policy-engine` 의 leaf conformance + `cargo test -p policy-transition`. 축 1 은 추가로 `apply.rs`/`dispatch.rs` 가 컴파일되면 최상위 dispatch 완비가 보장된다.

## 7. 출처 (실측 symbol)

- `crates/policy-server/asset-model/action/src/lib.rs` — `ActionBody` (`#[serde(tag="domain")]`, Tsify), domain newtype wrap, Multicall/Unknown inline, `ActionMeta`/`ActionNature`
- `crates/policy-server/asset-model/action/src/token/mod.rs` — `TokenAction` (`#[serde(tag="action")]`), `action_tag()` exhaustive match, `pub mod`/`pub use` 패턴
- `crates/policy-server/asset-model/action/src/{perp/open.rs, token/erc20_approve.rs}` — struct derive + `LiveField<T>` 유무 + `#[tsify(type=...)]`
- `crates/policy-server/asset-model/transition/src/apply.rs` — `trait Reducer`, `impl Reducer for ActionBody` exhaustive match
- `crates/policy-server/asset-model/transition/src/effect/{token.rs, perp/mod.rs}` — `impl Reducer for <Domain>Action` exhaustive match (단일파일/디렉토리 비대칭)
- `crates/policy-engine/src/lowering_v2/dispatch.rs` — `LoweredAction`, `lower_action` exhaustive match, `lowered()` helper
- `crates/policy-engine/src/lowering_v2/<domain>/mod.rs` — per-action dispatch + 각 domain 자체 `test_support::assert_conforms` 게이트 (token/perp/lending/amm/launchpad/airdrop 6곳)
- `crates/policy-engine/src/schema/mod.rs` — `include_str!` const + `SHIPPED_SCHEMA_FILES` 수동 등록, `merge_namespace_blocks`
- `crates/adapters/mappers/src/declarative/action_builder.rs` — `flatten_body` generic 추출
- `schema/policy-schema/actions/<domain>/<sub>.cedarschema` + `core.cedarschema` — Cedar 4.10 namespace
- B.3 deferred finding — `replicated-noodling-sprout.md` §12.8 / `SCHEMA_EXTENSION_PROPOSALS.md` (둘 다 gitignored `docs/`, fresh clone 부재 — optional)
