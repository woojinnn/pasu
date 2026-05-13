# Action 재정의 및 Adapter Modular 리팩토링 설계

작성일: 2026-05-13
대상 브랜치: `main` (작업은 별도 feature 브랜치 권장)
범위: `crates/policy-engine`, `crates/adapters/*` (구조 재편성), `crates/web-server`, `crates/policy_engine_wasm`, `extension/`, `policies/`, `policy-schema/`

> **Crate 재배치 결정 (2026-05-13 사용자 지시)**: Adapter 관련 로직은 모두 `crates/adapters/`를 **컨테이너 디렉토리**로 두고 그 아래 sub-crate로 둔다.
>
> - 기존 `crates/abi-resolver/` → `crates/adapters/abi-resolver/` 로 이동
> - 기존 `crates/mappers/` → `crates/adapters/mappers/` 로 이동
> - 기존 `crates/sign-resolver/` → `crates/adapters/sign-resolver/` 로 이동
> - 기존 `crates/request-router/` → `crates/adapters/request-router/` 로 이동 (adapter 스택의 top-level orchestrator)
> - 기존 `crates/adapters/{eip2612,permit2,uniswap-v2,uniswap-v3,universal-router}/` 5개 sub-crate는 **삭제** (로직만 위 sub-crate들로 흡수)
> - 기존 `crates/adapters-bundle/`도 **삭제**
>
> Cargo workspace `members` 경로는 `crates/adapters/{abi-resolver,mappers,sign-resolver,request-router}` 4개로 갱신. crate **이름**(`Cargo.toml`의 `package.name`)은 기존과 동일 유지 (`abi-resolver`, `mappers`, `sign-resolver`, `request-router`) — 외부 의존성 import 경로(`use abi_resolver::...`, `use request_router::...`)를 건드리지 않기 위함.

---

## 1. 목적과 배경

### 1.1 문제
- `crates/policy-engine/src/core.rs`의 `Action` enum이 5-variant (`Dex` / `Other` / `Permit2` / `Eip2612` / `Eip712Other`) 단일 평면 표현. `schema/` 디렉토리에 정형화된 32개 action JSON 정의가 별도로 존재하지만 Rust 코드와 단절.
- `crates/adapters/{eip2612,permit2,uniswap-v2,uniswap-v3,universal-router}` 5개 crate가 "calldata/signature → Action" 변환을 monolithic하게 담당. 디코딩(파라미터 추출)과 매핑(Action 생성)이 한 모듈에 섞여 있어 protocol 추가 시 boilerplate가 커지고 재사용이 어려움.
- 동일한 동작(예: Uniswap V2 swap calldata 디코드)을 `mappers/uniswap_v2`도 별개로 구현 — 중복.
- Action 정의가 두 곳에서 발산: `core::Action` (Cedar 평가용, 5-variant) vs `mappers::types::ActionFields` (스키마 기반, 6-variant). 같은 swap을 서로 다른 모양으로 표현.
- web-server는 `mappers::types::RootRequest`를, browser extension WASM은 `core::Action`을 wire format으로 사용 — 같은 의미 데이터에 두 표현.

### 1.2 목표 (한 줄)
> **Action을 32-variant 통합 타입으로 재정의하고, `Decoder → Mapper → Action` / `SignResolver → SignAdapter → Action` 의 두 modular pipeline으로 변환 경로를 정리한다. 정책 평가는 일단 Swap에만 적용한다.**

### 1.3 비목표
- Cedar 정책 DSL 자체의 재설계 (그대로 둠)
- Schema의 의미 변경 (필드 추가/제거 없음 — 단순 포팅)
- 새 protocol 추가 (포팅 후 별도 PR에서)
- Oracle/Portfolio enrichment 파이프라인 재설계 (그대로 둠)

---

## 2. 핵심 설계 결정

### 2.1 Action 통합 타입 (32-variant)

`crates/policy-engine/src/action/` 모듈로 분리. `core.rs`는 이미 ~1000라인이므로 더 키우지 않고 새 디렉토리로 옮긴다.

```
crates/policy-engine/src/
├── action/
│   ├── mod.rs              # Action enum, Category, ActionEnvelope re-export
│   ├── envelope.rs         # ActionEnvelope { action, category, fields }
│   ├── common.rs           # Address, AssetRef, AmountConstraint, Validity, DecimalString, UsdValuation 등
│   ├── dex.rs              # 7 actions: Swap, AddLiquidity, RemoveLiquidity, MintLiquidityNft, ...
│   ├── lending.rs          # 9 actions: Supply, Withdraw, Borrow, Repay, Liquidate, ...
│   ├── misc.rs             # 10 actions: Wrap, Unwrap, Approve, SetApprovalForAll, Transfer, Permit, ...
│   ├── staking.rs          # 3 actions: Stake, RequestUnstake, ClaimUnstake
│   └── restaking.rs        # 3 actions: Restake, RequestRestakeWithdrawal, ClaimRestakeWithdrawal
├── root.rs                 # RootRequest (top-level envelope: schemaVersion, chainId, from/to, actions[])
├── core.rs                 # 기존 도메인 타입은 유지하되 Action enum은 action/ 로 이동
└── ...
```

#### 2.1.1 Action enum 모양

스키마는 `action`(discriminator)과 `category`(직교 차원)를 분리한다. 같은 swap이 dex/liquid_staking/rwa 카테고리 모두에 등장할 수 있음. 따라서 Rust 표현도 두 차원을 분리한다.

```rust
// action/envelope.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Dex, Lending, Rwa, LiquidStaking, Restaking, Yield, Misc, Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "fields", rename_all = "snake_case")]
pub enum Action {
    // dex
    Swap(SwapAction),
    AddLiquidity(AddLiquidityAction),
    RemoveLiquidity(RemoveLiquidityAction),
    MintLiquidityNft(MintLiquidityNftAction),
    BurnLiquidityNft(BurnLiquidityNftAction),
    IncreaseLiquidity(IncreaseLiquidityAction),
    DecreaseLiquidity(DecreaseLiquidityAction),
    // lending
    Supply(SupplyAction),
    Withdraw(WithdrawAction),
    Borrow(BorrowAction),
    Repay(RepayAction),
    Liquidate(LiquidateAction),
    FlashLoan(FlashLoanAction),
    SetAuthorization(SetAuthorizationAction),
    SignAuthorization(SignAuthorizationAction),
    Revoke(RevokeAction),
    // misc
    Wrap(WrapAction),
    Unwrap(UnwrapAction),
    Approve(ApproveAction),
    SetApprovalForAll(SetApprovalForAllAction),
    Transfer(TransferAction),
    Permit(PermitAction),
    ClaimRewards(ClaimRewardsAction),
    SignMessage(SignMessageAction),
    Delegate(DelegateAction),
    Vote(VoteAction),
    // staking
    Stake(StakeAction),
    RequestUnstake(RequestUnstakeAction),
    ClaimUnstake(ClaimUnstakeAction),
    // restaking
    Restake(RestakeAction),
    RequestRestakeWithdrawal(RequestRestakeWithdrawalAction),
    ClaimRestakeWithdrawal(ClaimRestakeWithdrawalAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub category: Category,
    #[serde(flatten)]
    pub action: Action,   // serializes as { "action": "...", "fields": {...} }
}
```

JSON 출력 예시:
```json
{
  "category": "dex",
  "action": "swap",
  "fields": { "mode": "exact_in", "tokenIn": {...}, ... }
}
```

이 모양은 `schema/root.json`의 `ActionEnvelope` 정의와 정확히 일치한다.

#### 2.1.2 추가 정보 흡수 (Mapper-derived info를 Action에 포함)

현재 `mappers::types::SwapAction`이 schema보다 더 풍부한 정보를 가짐:
- `valueInUsd: UsdValuation`
- `minValueOutUsd: UsdValuation`
- `expectedValueOutUsd: UsdValuation`
- `deadlineSecondsFromNow: i64` (schema의 Validity와 중복 — Validity로 통일)

USD valuation은 schema 가이드라인(§2.3)에 따르면 "Oracle/Portfolio enrichment 단계의 데이터로 schema 표면에 부재"이지만, **사용자 요청은 "Mapper가 추가로 제공하는 정보는 Action에 포함시킨다"** 이므로 **별도 `enrichment` 영역으로 묶어서** Action 구조에 포함시킨다.

각 Action variant struct에 optional `enrichment` 필드:

```rust
// action/dex.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapAction {
    // ─── schema/actions/dex/swap.json 필드 (필수: mode, tokenIn, tokenOut, amountIn, amountOut, recipient) ───
    pub mode: SwapMode,
    pub token_in: AssetRef,
    pub token_out: AssetRef,
    pub amount_in: AmountConstraint,
    pub amount_out: AmountConstraint,
    pub recipient: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u32>,

    // ─── Mapper-derived enrichment (schema에는 없음, host:oracle/quote 출처) ───
    #[serde(default, skip_serializing_if = "SwapEnrichment::is_empty")]
    pub enrichment: SwapEnrichment,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapEnrichment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_usd: Option<UsdValuation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value_out_usd: Option<UsdValuation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_value_out_usd: Option<UsdValuation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance_covers_input: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fraction_of_portfolio_bps: Option<u32>,
}
```

`enrichment`는 Mapper가 처음에 빈 값으로 만들고, host enrichment 단계에서 채운다. 다른 action variant들도 동일한 패턴 (`PermitEnrichment`, `ApproveEnrichment` 등 필요한 것만). 대부분의 variant는 enrichment 없이 schema 필드만으로 충분.

#### 2.1.3 공용 primitives

```rust
// action/common.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(String);  // newtype: lowercase 0x + 40 hex 검증

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecimalString(String);  // u256 decimal 검증

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    pub kind: AssetKind,
    pub chain_id: u64,                          // 항상 채워짐
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,                 // host:registry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,                   // host:registry
}

pub enum AssetKind { Native, Erc20, Erc721, Erc1155, Unknown }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountConstraint {
    pub kind: AmountKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<DecimalString>,
}

pub enum AmountKind { Exact, Min, Max, Unlimited, Estimated, Unknown }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Validity {
    pub expires_at: DecimalString,
    pub source: ValiditySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValiditySource { TxDeadline, SignatureDeadline, GrantExpiration }
```

### 2.2 Decoder / Mapper / SignAdapter trait 3종

#### 2.2.1 Decoder trait — `crates/adapters/abi-resolver/src/decoder.rs`

```rust
/// Calldata → 구조화된 디코딩 결과.
/// 한 Decoder는 하나의 protocol+function (또는 한 protocol의 모든 entry point) 담당.
pub trait Decoder: Send + Sync {
    fn id(&self) -> DecoderId;            // e.g. "uniswap-v2/swapExactTokensForTokens"
    fn match_keys(&self) -> Vec<DecoderMatchKey>;  // (chain_id, address, selector)
    fn decode(&self, ctx: &DecodeContext, tx: &TransactionRequest)
        -> Result<DecodedCall, DecoderError>;
}

pub struct DecodedCall {
    pub decoder_id: DecoderId,
    pub function_signature: String,
    pub args: Vec<DecodedArg>,            // 순서 보존
    pub nested: Vec<DecodedCall>,         // multicall / universal-router 등 nested 호출
}

pub struct DecodedArg {
    pub name: String,
    pub abi_type: String,                  // "address", "uint256", "address[]" 등
    pub value: DecodedValue,
}

pub enum DecodedValue {
    Address(Address),
    Uint(U256),
    Int(I256),
    Bool(bool),
    Bytes(Vec<u8>),
    String(String),
    Array(Vec<DecodedValue>),
    Tuple(Vec<DecodedValue>),
}
```

Registry: `DecoderRegistry` (match_keys 기반 lookup, 기존 `Resolver` 구현 위에 trait dispatch 추가).

#### 2.2.2 Mapper trait — `crates/adapters/mappers/src/mapper.rs`

```rust
/// Decoded calldata → 의미 단위 Action(s).
/// 한 Mapper는 한 protocol+function 담당. 한 호출이 여러 Action을 낳을 수 있음 (composite router 등).
pub trait Mapper: Send + Sync {
    fn id(&self) -> MapperId;
    fn accepts(&self, decoded: &DecodedCall) -> bool;
    fn map(&self, ctx: &MapContext, decoded: &DecodedCall)
        -> Result<Vec<ActionEnvelope>, MapperError>;
}

pub struct MapContext<'a> {
    pub chain_id: u64,
    pub from: &'a Address,
    pub to: &'a Address,
    pub value_wei: &'a DecimalString,
    pub block_timestamp: Option<u64>,
    pub token_registry: &'a dyn TokenRegistry,
}
```

Registry: `MapperRegistry`. Decoder가 nested call 트리를 만들면 Mapper도 트리를 따라 재귀 매핑 가능.

#### 2.2.3 SignAdapter trait — `crates/adapters/sign-resolver/src/sign_adapter.rs`

```rust
/// 서명 요청 → 의미 단위 Action.
pub trait SignAdapter: Send + Sync {
    fn id(&self) -> SignAdapterId;
    fn match_keys(&self) -> Vec<SignMatchKey>;   // (chain_id, verifyingContract, primaryType)
    fn map(&self, ctx: &SignMapContext, sig: &SignRequest)
        -> Result<Vec<ActionEnvelope>, SignAdapterError>;
}
```

`SignRequest`는 기존 `sign-resolver::SignRequest` 그대로 (이미 잘 구조화됨). 서명용 Adapter는 EIP-2612 → `Action::Permit`, Permit2 PermitSingle/PermitBatch → `Action::Permit` 또는 `Action::Approve`로 매핑.

#### 2.2.4 trait 공통 표면 — `crates/policy-engine/src/adapter.rs`

기존 `TransactionActionAdapter` / `SignatureActionAdapter` trait은 **삭제**. 대신 Pipeline 레벨에서 Decoder/Mapper/SignAdapter를 직접 호출. policy-engine은 trait들을 모르고, request-router가 조립.

```
request-router::route(request):
  match request {
    Transaction(tx) =>
      let decoded = decoder_registry.resolve(tx)?
      let actions = mapper_registry.map(decoded)?
      actions
    Sign(sig) =>
      let actions = sign_adapter_registry.map(sig)?
      actions
  }
```

이 분리로 policy-engine은 순수하게 "Action → Verdict"만 담당하게 됨.

### 2.3 Adapter crate 재배치 + 옛 sub-crate 삭제

최종 디렉토리 구조:

```
crates/
├── adapters/
│   ├── abi-resolver/          # 기존 crates/abi-resolver/ 이동 + Decoder trait 신설
│   │   └── src/
│   │       ├── decoder.rs              # Decoder trait + DecoderRegistry
│   │       ├── decoders/               # protocol-별 Decoder 구현
│   │       │   ├── uniswap_v2.rs
│   │       │   ├── uniswap_v3.rs
│   │       │   ├── uniswap_v4.rs
│   │       │   └── universal_router.rs
│   │       ├── resolver.rs             # 기존 Sourcify/OpenChain 폴백 (그대로)
│   │       ├── subdecode/              # 기존 nested calldata 파서 (그대로)
│   │       └── lib.rs
│   ├── mappers/               # 기존 crates/mappers/ 이동 + Mapper trait 신설
│   │   └── src/
│   │       ├── mapper.rs               # Mapper trait + MapperRegistry
│   │       ├── protocols/              # protocol-별 Mapper 구현
│   │       │   ├── uniswap_v2/
│   │       │   ├── uniswap_v3/
│   │       │   ├── uniswap_v4/
│   │       │   └── universal_router/
│   │       ├── context.rs
│   │       └── lib.rs
│   ├── sign-resolver/         # 기존 crates/sign-resolver/ 이동 + SignAdapter trait 신설
│   │   └── src/
│   │       ├── sign_adapter.rs         # SignAdapter trait + SignAdapterRegistry
│   │       ├── adapters/               # signature-별 SignAdapter 구현
│   │       │   ├── eip2612.rs          # 옛 crates/adapters/eip2612 의 로직
│   │       │   └── permit2.rs          # 옛 crates/adapters/permit2 의 로직
│   │       ├── method.rs               # 기존 SignMethod (그대로)
│   │       ├── payload.rs              # 기존 SignPayload (그대로)
│   │       └── lib.rs
│   └── request-router/        # 기존 crates/request-router/ 이동. Adapter 스택 top-level orchestrator
│       └── src/
│           ├── lib.rs                  # route_request(method, params, chain_id) -> RootRequest
│           ├── transaction.rs          # eth_sendTransaction / eth_call 류
│           ├── signature.rs            # eth_signTypedData_v4 / personal_sign / eth_sign
│           ├── user_operation.rs       # eth_sendUserOperation (ERC-4337)
│           └── error.rs
├── policy-engine/
├── policy_engine_wasm/
├── integration-tests/
└── web-server/
```

이동 매핑:

| 출처 | 도착지 | 비고 |
|---|---|---|
| `crates/abi-resolver/` (전체) | `crates/adapters/abi-resolver/` | `git mv` |
| `crates/mappers/` (전체) | `crates/adapters/mappers/` | `git mv` |
| `crates/sign-resolver/` (전체) | `crates/adapters/sign-resolver/` | `git mv` |
| `crates/request-router/` (전체) | `crates/adapters/request-router/` | `git mv` |
| `crates/adapters/eip2612/src/*.rs` | `crates/adapters/sign-resolver/src/adapters/eip2612.rs` 로 흡수 | SignAdapter trait 구현으로 재작성 |
| `crates/adapters/permit2/src/*.rs` | `crates/adapters/sign-resolver/src/adapters/permit2.rs` 로 흡수 | SignAdapter trait 구현으로 재작성 |
| `crates/adapters/uniswap-v2/src/*.rs` | 디코드는 `crates/adapters/abi-resolver/src/decoders/uniswap_v2.rs`, 매핑은 `crates/adapters/mappers/src/protocols/uniswap_v2/` | 둘로 분할 |
| `crates/adapters/uniswap-v3/src/*.rs` | 동일하게 분할 | 동일 |
| `crates/adapters/universal-router/src/*.rs` | 디코드는 abi-resolver 의 command-stream 파서로, 매핑은 mappers 로 | 분할 |
| `crates/adapters-bundle/` | 삭제 | 역할 사라짐 (registry 조립은 `policy_engine_wasm` 또는 `request-router` 에서) |

Cargo workspace `Cargo.toml` 변경:

```toml
[workspace]
members = [
    "crates/adapters/abi-resolver",
    "crates/adapters/mappers",
    "crates/adapters/sign-resolver",
    "crates/adapters/request-router",
    "crates/policy-engine",
    "crates/policy_engine_wasm",
    "crates/integration-tests",
    "crates/web-server",
]
```

crate **이름**은 기존 그대로 (`abi-resolver`, `mappers`, `sign-resolver`, `request-router`) → 다른 crate 의 `Cargo.toml` 의존성 선언 (`abi-resolver = { path = ... }`) 에서 path 만 갱신, name 변경 없음. `use abi_resolver::Resolver`, `use request_router::route_request` 같은 import 코드는 변경 없음.

### 2.4 PolicyRequest = Swap-only 단순화

- `crates/policy-engine/src/policy.rs`의 lowering은 `Action::Swap`만 처리 (기존 `enrich_dex_action`, `request_from_action` 중 Dex 경로만 살림)
- 기타 31개 Action variant는 `PolicyRequest`로 lowering되지 않음 — Pipeline에서 무조건 `Verdict::Pass`로 통과 (또는 `Verdict::Unsupported` 새 variant)
- `policy-schema/actions/`: `dex.cedarschema`만 유지. `eip2612.cedarschema`, `eip712_other.cedarschema`, `other.cedarschema`, `permit2.cedarschema`, `signature_base.cedarschema` 삭제
- `policy-schema/core.cedarschema`는 Wallet/Protocol/Token 같은 base type이므로 유지
- `policies/dex/`의 10개 .cedar 그대로 유지
- `policies/signature/` 전체 삭제 (`_shared/`, `eip2612/`, `eip712-other/`, `permit2/`)
- `dex.cedarschema`의 `DexContext`는 swap-aggregate 모양인데 (multi-hop, Set<Token>), 새 Action::Swap은 single-hop이므로 lowering 시 single token을 Set으로 감싸서 호환 유지. cedarschema 자체 수정은 본 PR 범위 외 (사용자가 추후 수정 예정)

### 2.5 web-server / extension / WASM 인터페이스

#### 2.5.1 wire format 통일
`crates/policy_engine_wasm`의 export 함수가 반환하는 JSON과 `crates/web-server/POST /api/decode`가 반환하는 JSON 중 **action-부분** 을 동일하게 만든다.

- 둘 다 `RootRequest { ..., actions: [ActionEnvelope] }` 모양 반환
- WASM은 추가로 verdict까지 (`{ root: RootRequest, verdict: Verdict }`)
- web-server `/api/decode`도 verdict 옵션으로 포함 (extension 측에서 한 번에 받을 수 있게)

#### 2.5.2 extension TS 타입 동기화
- `extension/.../wasm-bridge.types.ts`의 parser를 새 32-variant ActionEnvelope에 맞게 재작성
- Discriminator는 `category + action` 2단계 — `category`로 grouping하고 `action`으로 variant 확정
- 기존 `dex / other / permit2 / eip2612 / eip712Other` parser는 deprecated → 새 parser로 교체
- TS 타입은 schemars/typescript codegen으로 자동 생성 검토 (`crates/policy-engine`에서 export)

#### 2.5.3 web-server ↔ extension 중복 제거
현재 둘 다 abi-resolver + mappers를 호출하는 entry point를 각자 만들고 있음. 본 리팩토링 후:

```
[extension JS] ──→ wasm.route_request(method, params, chain_id)
                         │
                         ▼
                  policy_engine_wasm::api
                         │
                         ▼
        crates/adapters/request-router::route_request(...)  ← 공용 진입점
                         │
              ┌──────────┼──────────┐
              ▼          ▼          ▼
        abi-resolver  mappers   sign-resolver
        (Decoder)    (Mapper)   (SignAdapter)
                         │
                         ▼
                   ActionEnvelope[]

[web-server HTTP] POST /api/decode
                         │
                         ▼
        crates/adapters/request-router::route_request(...)  ← 같은 진입점
```

`request_router::route_request(method, params, chain_id) -> RootRequest` 단일 함수로 통일. WASM과 web-server 모두 이 함수를 호출. 호출자만 다르고 변환 로직은 0% 중복. policy-engine 은 ActionEnvelope 만 받음 — RPC parsing 책임 없음.

### 2.6 CI 통합

`.github/workflows/ci.yml`에 다음 추가:
1. `cargo test -p web-server` — web-server 단위/통합 테스트 실행
2. 신규 `crates/web-server/tests/http_integration.rs` — axum test server 띄워 POST /api/decode/sign 호출, JSON 응답 schema 검증
3. `cd extension && npm ci && npm run typecheck && npm run build` — extension 빌드 + 타입 체크
4. `cargo deny check` — dependency 검사 (기존 deny.toml 활용)
5. Golden vector 회귀 — `crates/integration-tests/data/golden/`에 새 ActionEnvelope JSON snapshot 추가, `insta` 또는 단순 JSON 비교로 회귀 감지

---

## 3. 마이그레이션 단계 (Codex 위임 단위)

각 단계는 별도 PR + 별도 Codex 위임 task. 단계 사이에 사용자 검수.

### Phase 0 — Baseline 스냅샷 (사용자 직접)
- 현재 web-server 응답을 sample calldata 10종에 대해 capture → `tests/golden/legacy/` (PR 직전 dump)
- 현재 WASM build_action_for_request_json 응답 동일 capture
- 향후 Phase 7에서 회귀 비교 baseline으로 사용

### Phase 1 — Action 통합 타입 도입 (Codex)
- 새 `crates/policy-engine/src/action/` 모듈 32 variant
- `crates/policy-engine/src/root.rs` `RootRequest`
- 공용 primitives (`Address` newtype, `AssetRef`, `AmountConstraint`, `Validity`, `DecimalString` 등)
- **기존 `core::Action`은 일단 그대로 유지** (`LegacyAction`으로 이름 변경 검토). 빌드는 그대로 통과해야 함.
- 신규 타입 단위 테스트 (각 action JSON sample을 Rust struct로 round-trip)

### Phase 1.5 — Crate 위치 재배치 (Codex) ⭐ 추가
- `git mv crates/abi-resolver crates/adapters/abi-resolver`
- `git mv crates/mappers crates/adapters/mappers`
- `git mv crates/sign-resolver crates/adapters/sign-resolver`
- `git mv crates/request-router crates/adapters/request-router`
- 루트 `Cargo.toml`의 `[workspace] members` 경로 갱신 (4개 path)
- 각 sub-crate를 의존하는 다른 crate (`Cargo.toml`의 `path = ...`) 경로 갱신:
  - `crates/policy-engine/Cargo.toml`
  - `crates/policy_engine_wasm/Cargo.toml`
  - `crates/integration-tests/Cargo.toml`
  - `crates/web-server/Cargo.toml`
  - 기존 `crates/adapters/{eip2612,permit2,uniswap-v2,uniswap-v3,universal-router}/Cargo.toml` (아직 살아 있을 동안만)
- `cargo build --workspace`, `cargo test --workspace` 통과 확인
- crate **이름**은 변경 없음, import 경로 그대로

### Phase 2 — Decoder trait + abi-resolver 재정비 (Codex)
- 위치: `crates/adapters/abi-resolver/src/decoder.rs`, `crates/adapters/abi-resolver/src/decoders/`
- `Decoder` trait + `DecodedCall` 정의
- `DecoderRegistry` (match_keys 기반)
- 기존 `subdecode/`의 protocol-specific 디코딩 로직을 trait 구현으로 wrap
- `Resolver::resolve`는 그대로 두되 내부에서 `DecoderRegistry` 호출하도록 변경 (외부 API 호환)
- Uniswap V2 / V3 / V4 / Universal Router 4개 Decoder 구현

### Phase 3 — Mapper trait + mappers 재정비 (Codex)
- 위치: `crates/adapters/mappers/src/mapper.rs`, `crates/adapters/mappers/src/protocols/`
- `Mapper` trait + `MapContext`
- `MapperRegistry`
- `mappers::types`를 삭제하고 `policy-engine::action`을 import
- 기존 `mappers/protocols/{uniswap_v2,uniswap_v3,uniswap_v4,universal_router}` 모듈을 Mapper trait 구현으로 변환
- 각 Mapper의 출력 ActionEnvelope이 schema JSON과 round-trip 가능한지 테스트

### Phase 4 — SignAdapter trait + sign-resolver 재정비 (Codex)
- 위치: `crates/adapters/sign-resolver/src/sign_adapter.rs`, `crates/adapters/sign-resolver/src/adapters/`
- `SignAdapter` trait
- `SignAdapterRegistry`
- 옛 `crates/adapters/eip2612` 로직 → `crates/adapters/sign-resolver/src/adapters/eip2612.rs`로 이전 (SignAdapter trait 구현, 출력 `Action::Permit`)
- 옛 `crates/adapters/permit2` 로직 → `crates/adapters/sign-resolver/src/adapters/permit2.rs`로 동일 이전

### Phase 5 — 옛 adapter sub-crate 삭제 (Codex)
- `crates/adapters/{eip2612,permit2,uniswap-v2,uniswap-v3,universal-router}/` 5개 sub-crate 디렉토리 삭제
- `crates/adapters-bundle/` 삭제 (workspace member 에서도 제거)
- 루트 `Cargo.toml` workspace `members` 정리 (이미 옮긴 abi-resolver/mappers/sign-resolver 3개만 adapters/ 하위에 남도록)
- `policy-engine::adapter` module에서 `TransactionActionAdapter` / `SignatureActionAdapter` trait 삭제
- request-router가 새 registry 3종 (`DecoderRegistry`, `MapperRegistry`, `SignAdapterRegistry`)을 직접 사용하도록 변경

### Phase 6 — Pipeline & PolicyRequest Swap-only (Codex)
- `policy.rs`의 `request_from_action`을 `Action::Swap`만 처리하도록 단순화
- `Action::Swap` 외의 variant는 Pipeline에서 `Verdict::Pass` (또는 새 `Verdict::Unsupported`) 반환
- `policy-schema/actions/` 정리 (dex.cedarschema만 남김)
- `policies/signature/` 디렉토리 삭제
- 기존 `core::Action` (LegacyAction) 완전 제거 — 신규 `Action` 단일 사용

### Phase 7 — web-server / WASM 인터페이스 통일 (Codex)
- `crates/policy_engine_wasm`의 export 함수 시그니처를 `RootRequest` 반환으로 변경
- `crates/web-server`의 `/api/decode` / `/api/sign` 응답을 `RootRequest`로 통일
- `request-router::route_transaction()` / `route_signature()` 두 함수로 통일된 진입점 제공
- WASM과 web-server 모두 이 두 함수를 호출
- web-server 통합 테스트 (`tests/http_integration.rs`) 추가 — Phase 0 baseline과 비교하여 동등성 확인

### Phase 8 — extension TS 동기화 (Codex + 수동)
- `extension/`의 `wasm-bridge.types.ts` 파서를 새 32-variant에 맞게 재작성
- TS 타입은 가능하면 `crates/policy-engine`에서 `schemars` + `ts-rs`로 자동 생성 (검토)
- extension `vitest` 테스트 갱신
- chrome/firefox 빌드 검증

### Phase 9 — CI 통합 (Codex)
- `.github/workflows/ci.yml`에 다음 job 추가:
  - `cargo test -p web-server`
  - `cargo test -p integration-tests`
  - `cd extension && npm ci && npm run typecheck && npm run build && npm test`
  - Golden vector 회귀 테스트
- 빌드 시간 영향 측정 (5분 이내 유지 목표)

---

## 4. 위험 / 완화

| 위험 | 영향 | 완화 |
|---|---|---|
| Action 32 variants가 너무 커서 enum size가 큼 (`#[allow(clippy::large_enum_variant)]` 필요) | 메모리 + Move cost | `Box<>` wrap 검토. 핫패스(Swap)는 직접 둠. 나머지는 Box. |
| Cedar `DexContext`가 multi-hop Set<Token> 모양인데 새 `SwapAction`은 single-hop | Cedar 평가 시 lowering 비용 / 호환성 | Lowering에서 single token을 1-element Set으로 감싸 호환 유지. cedarschema 수정은 별도 PR (사용자 추후). |
| Extension WASM wire format 변경 → 브라우저 익스텐션 backward-incompatibility | 사용자 빌드 깨짐 | Phase 0 baseline JSON dump → Phase 7 비교 회귀 테스트로 차이 명시 → TS 타입 atomic 갱신 (Phase 8) → version bump 명시 |
| 32 variants × 32 mappers 작성 비용 폭증 | 일정 지연 | 본 PR은 변환 SCAFFOLD까지만. 기존에 매핑되어 있던 6 actions (swap/wrap/unwrap/approve/add_liquidity/remove_liquidity) 만 fully wired. 나머지 26 variants는 타입 정의만 도입, 매퍼는 후속 PR에서 추가. |
| Phase 7에서 web-server JSON 응답 모양 변경 | 외부 통합 깨짐 | Phase 0 baseline과 diff 첨부. 변경되는 필드 목록 명시. |
| Codex가 큰 PR을 다루기 어려움 | review 지연, regression | 각 phase = 별도 PR. 단계 사이에 사용자 manual smoke test. |
| 마이그레이션 중 일시적 dead code (LegacyAction 잔존) | Lint failure | `#[allow(dead_code)]` 일시 적용, Phase 6에서 완전 제거 |

---

## 5. 성공 기준

작업 완료 판단 기준 (모두 충족):

- [ ] `cargo build --workspace` 통과
- [ ] `cargo test --workspace` 통과 (기존 + 새 통합 테스트)
- [ ] `cargo clippy --workspace -- -D warnings` 통과
- [ ] `cargo deny check` 통과
- [ ] extension `npm run typecheck && npm run build && npm test` 통과
- [ ] web-server `tests/http_integration.rs` Phase 0 baseline과 swap calldata 10종에 대해 JSON-equivalent 응답 (token symbol/decimals 같은 host:registry 필드 제외)
- [ ] `crates/adapters/`는 `abi-resolver/`, `mappers/`, `sign-resolver/`, `request-router/` 4개 sub-crate만 포함 (그 외 sub-crate 없음). `crates/adapters-bundle/` 디렉토리 존재 X. `crates/abi-resolver/`, `crates/mappers/`, `crates/sign-resolver/`, `crates/request-router/` 모두 존재 X (전부 adapters/ 하위로 이동됨)
- [ ] `policies/signature/` 디렉토리 존재 X
- [ ] `Action` 정의가 `crates/policy-engine/src/action/` 한 곳에만 존재 (grep으로 검증)
- [ ] Cedar swap policy 10종 (`policies/dex/*.cedar`) 모두 통과/실패 케이스 단위 테스트 그대로 통과
- [ ] CI 신규 job 모두 green
- [ ] 본 design doc과 코드 차이 없음 (구조적으로)

---

## 6. 결정이 미뤄진 항목 / 후속

- **cedarschema 재설계**: 사용자가 추후 별도 작업. 새 single-hop SwapAction에 맞춰 `DexContext`를 단순화 (Set<Token> → Token, totalInputUsd → inputUsd 등)
- **나머지 26 actions의 mapper 구현**: 본 PR 범위 외. 별도 후속 PR에서 protocol 단위로 추가.
- **`schemars` + `ts-rs` 자동 codegen**: Phase 8에서 시도해보고 어렵다면 수동 sync.
- **enrichment 데이터의 host enrichment pipeline 재설계**: 현재 `enrich_dex_action`을 generalized로 확장할지 검토 — 본 PR 범위 외, 별도 작업.

---

## 7. 참고 자료

- `schema/CLAUDE.md` — schema 설계 원칙 (white-list, universal field, action ⊥ category)
- `schema/schema/actions/` — 32 action JSON 정의
- `schema/schema/common/_common.json` — 공용 primitive 정의
- `schema/schema/root.json` — RootRequest envelope 정의
- `crates/policy-engine/src/core.rs` — 기존 Action / 도메인 타입
- `crates/policy-engine/src/policy.rs` — 기존 lowering / Cedar evaluator
- `crates/mappers/src/types/` — 기존 부분 포팅 (6 actions)
- `policy-schema/actions/dex.cedarschema` — DEX(swap) Cedar context
- `policies/dex/*.cedar` — 유지 대상 swap policy 10종
