# ScopeBall Protocol-Agnostic Onboarding Framework

> 목적: 새 프로토콜마다 다시 생각하지 않도록, **프로토콜 독립 코드 골격과 실행 인스트럭션**을 고정한다. 이 문서는 구현 전체를 한 번에 끝내는 계획이 아니라, 어떤 프로토콜에도 반복 적용할 수 있는 framework contract 이다.
>
> 범위: V3 `ActionBody[]` 디코드 경로만. 레거시 `ActionEnvelope` 경로는 고려하지 않는다.

---

## 0. 목표 모델

프로토콜 온보딩 완료는 “테스트가 green” 이 아니다. 완료는 아래 네 층이 모두 증명된 상태다.

| 층 | 질문 | 현재 근거 | 부족하면 |
|---|---|---|---|
| **Surface** | user-facing contract/function/signature 를 빠뜨리지 않았나? | `registryV2/surface/**` + `check:surface` | `_deployments.json`, ABI snapshot, coverage 보강 |
| **Shape** | manifest 가 실제 `ActionBody` 타입으로 round-trip 되나? | `v3-harness validate`, `oracle.rs` typed round-trip | manifest/Tier3 schema/lowering 수정 |
| **Semantic** | token, amount, recipient, spender, pool, path, live source 가 맞나? | `expect_body`, projection, field-level golden | assertion/projection 추가 후 decoder 수정 |
| **Production path** | production WASM export 로 같은 결과가 나오나? | `route_calldata` / `route_typed_data` 직접 호출 | WASM/export/loader 경계 수정 |

중요한 원칙:

- **프로토콜별 특수 지식은 data artifact 로 격리**한다. 하니스 코드는 protocol-agnostic 이어야 한다.
- **정답 작성량은 selector 수에 비례**해야 한다. Tx 수가 늘어도 projection/assertion 작성량은 폭증하면 안 된다.
- **semantic-critical field 는 domain 만으로 통과시키지 않는다.** field assertion 이 없으면 “검증 안 됨”이다.
- **manifest 를 manifest 로 검증하지 않는다.** projection 은 raw ABI decode / independent parser / primary-source fact 에서 온다.
- **완료 기준은 opt-in strict 이다.** 기존 프로토콜을 한 번에 깨지 말고, protocol 단위로 strict 를 켠다.

---

## 1. Framework Code Skeleton

아래는 큰 틀의 코드 구조다. 처음부터 전부 구현하지 않아도 되지만, 새 기능은 이 경계를 따라 붙인다.

```text
crates/integration-tests/src/harness/
├─ corpus.rs              # existing: corpus replay + expect verdict
├─ oracle.rs              # existing: envelope/type/domain/error class
├─ semantic.rs            # implemented: generic expect_body assertion engine
├─ projection.rs          # planned: selector-level independent expected-field projection
├─ semantic_lints.rs      # planned: zero/unresolved/high-risk-field lints
├─ audit.rs               # planned: protocol-level strict audit aggregator
└─ fixtures.rs            # optional: reusable JSON pointer/action find helpers

crates/integration-tests/src/bin/v3_harness.rs
├─ corpus                 # existing; calls semantic assertions when present
├─ validate               # existing; single_emit now, strategy-aware later
├─ coverage               # existing
├─ import-*               # existing; normalizes RPC hex quantities
└─ audit                  # planned: protocol strict gate wrapper
```

### 1.1 `expect_body` data contract

`expect_body` 는 corpus entry 에 붙는 optional field-level assertion list 다. 없으면 기존 corpus 는 그대로 동작한다. 있으면 `expect:"pass"` 이후 반드시 검사한다.

```jsonc
{
  "expect": "pass",
  "expect_domain": "multicall",
  "expect_body": [
    {
      "path": "$.data.actions[0].body.actions[1].body.token_in.key.address",
      "op": "equals",
      "value": "0x4200000000000000000000000000000000000006"
    },
    {
      "path": "$.data.actions[0].body.actions[1].body.token_out.key.address",
      "op": "nonzero_address"
    },
    {
      "path": "$.data.actions[0].body.actions[1].body.venue.fee_tier_bp",
      "op": "equals",
      "value": 50
    }
  ]
}
```

Protocol-agnostic matcher set:

| op | 의미 |
|---|---|
| `exists` | JSON pointer/path 가 존재해야 함 |
| `absent` | 존재하면 실패 |
| `equals` | JSON scalar/object/array deep equality |
| `not_equals` | `value` 와 달라야 함 |
| `one_of` | 값이 `values[]` 중 하나 |
| `contains` | array/string contains |
| `len` | array/string length equals `value` |
| `nonzero_address` | `0x` + 40 hex 이고 all-zero 아님 |
| `hex_eq` | case-insensitive hex equality |
| `u256_hex_eq` | decimal/hex input 을 U256 numeric equality 로 비교 |

Rust skeleton:

```rust
#[derive(Debug, Deserialize)]
pub struct BodyAssertion {
    pub path: String,
    pub op: AssertionOp,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub values: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionOp {
    Exists,
    Absent,
    Equals,
    NotEquals,
    OneOf,
    Contains,
    Len,
    NonzeroAddress,
    HexEq,
    U256HexEq,
}

pub fn check_expect_body(envelope: &serde_json::Value, assertions: &[BodyAssertion]) -> Result<(), String>;
```

Implementation rule:

- `expect_body` 실패는 corpus outcome `matched=false`.
- failure detail 은 `path`, `op`, expected, actual 을 포함한다.
- path dialect 는 JSON Pointer(`/...`), `$` dotted/index(`$.data.actions[0]`), recursive field(`$..address`) 를 지원한다.
- assertion engine 은 `ActionBody` schema 를 몰라야 한다. JSON 만 본다.

### 1.2 Projection data contract

Projection 은 selector 단위 2nd-opinion 이다. 한 selector 에 대해 raw args 에서 기대 ActionBody field 를 계산한다.

```jsonc
{
  "selector": "0x095ea7b3",
  "signature": "approve(address spender,uint256 amount)",
  "scope": {
    "chains": [1, 8453],
    "addresses": ["*"]
  },
  "expect": [
    { "path": "$..domain", "op": "contains", "value": "token" },
    { "path": "$..spender", "op": "hex_eq", "from": "$raw.spender" },
    { "path": "$..amount", "op": "u256_hex_eq", "from": "$raw.amount" },
    { "path": "$..token.key.address", "op": "hex_eq", "from": "$tx.to" }
  ]
}
```

Projection source grammar:

| source | 의미 |
|---|---|
| `$tx.chain_id` / `$tx.to` / `$tx.value` / `$tx.from` | route input |
| `$raw.<arg>` | independent ABI decode result |
| `$raw.<arg>[i]` | decoded array/tuple element |
| `$derive.<name>(...)` | harness-owned independent derivation |
| literal | primary-source static fact |

Allowed `derive` examples:

- `uniswap_v3_path_first_token(path)`
- `uniswap_v3_path_last_token(path)`
- `uniswap_v3_path_first_fee(path)`
- `keccak_abi_tuple(...)`
- `curve_coin_at(pool_id, index)`
- `lower_hex(address)`

Non-circularity rule:

- Projection may read ABI signature and raw calldata.
- Projection may NOT read `emit.body`, manifest placeholder paths, or decoder output as its expected source.
- If expected value is computed from the same implementation path as production, it is not a projection. It is a duplicate decode smoke test.

### 1.3 Semantic lints

Semantic lints catch broad classes before protocol-specific assertions exist.

Initial generic lints:

| lint | applies to | default |
|---|---|---|
| `nonzero_asset_address` | token/amm/lending/liquid_staking/staking token refs | warn, strict=fail |
| `nonzero_permission_target` | spender/operator/authorized/delegatee | warn, strict=fail |
| `nonzero_pool_or_venue` | AMM venue pool, PoolManager, Comet/market IDs where address-like | warn, strict=fail |
| `no_unresolved_placeholder` | envelope telemetry once implemented | fail |
| `live_input_source_present` | action has non-empty live_inputs object | warn unless action documented raw-only |

Lints must support suppressions in corpus/projection:

```jsonc
"suppress_lints": [
  { "lint": "nonzero_asset_address", "path": "$..currency0", "reason": "Uniswap V4 native ETH sentinel" }
]
```

No suppression without reason.

### 1.4 Protocol audit command

Target CLI:

```bash
cargo run -p policy-engine-integration-tests --bin v3-harness -- audit \
  --protocol <protocol> \
  --strict
```

Audit stages:

1. build or verify `registryV2/index` freshness.
2. `check:surface --strict-protocol <protocol>`.
3. `validate --filter <protocol> --strategy all`.
4. `corpus --protocol <protocol> --expect-body --projections`.
5. semantic lints in strict mode.
6. coverage report: selectors with no real tx, no edge, no projection, no body assertions.
7. single JSON summary under `crates/integration-tests/logs/<protocol>/YYYY-MM-DD-audit.json`.

Audit bucket vocabulary:

| bucket | meaning | disposition |
|---|---|---|
| `correct` | routed + shape + semantic assertions/projection pass | done |
| `untested_semantic` | shape pass but no semantic oracle for critical fields | add `expect_body` or projection |
| `mis_decoded` | semantic assertion/projection fails | fix manifest/Tier2/Tier3 |
| `uncovered` | no mapper / Unknown where COVER expected | add manifest/wrapper |
| `decode_error` | hard builder/serde/decode failure | fix ABI/emit/engine |
| `excluded` | explicit non-user or out-of-scope | keep reason |

---

## 2. Protocol-Agnostic Artifact Layout

Every protocol uses the same paths.

```text
registryV2/surface/<protocol>/
├─ _deployments.json
├─ <contract>.abi.json
└─ <contract>.coverage.json

registryV2/manifests/<protocol>/<contract>/
└─ <function>@1.0.0.json

crates/integration-tests/data/golden/v3-decode/<protocol>/
├─ corpus.json
└─ projections/
   ├─ <selector-or-name>.json
   └─ ...

crates/integration-tests/logs/<protocol>/
└─ YYYY-MM-DD-<source-or-audit>.json
```

Do not create protocol-specific harness code unless the protocol truly needs a new independent derivation. Prefer data artifacts first.

---

## 3. End-to-End Onboarding Instructions

### P0. Contract Inventory

Goal: no user-facing contract is invisible.

Required artifacts:

- `registryV2/surface/<protocol>/_deployments.json`
- one `<contract>.abi.json` for every `cover` deployment
- one `<contract>.coverage.json` for every covered ABI surface

Steps:

1. Collect official deployment sources first.
   - official docs deployments/addresses page
   - official GitHub deploy artifacts
   - on-chain registry/address provider
   - verified explorer pages only as ABI/address proof
2. Challenge the list with secondary discovery.
   - DefiLlama adapter repo
   - Dune decoded namespace
   - Etherscan/Basescan labels
   - Sourcify verified repo
   - optional LLM discovery panel, candidate-only
3. For every deployed contract, mark:
   - `cover`: user/EOA/smart-account can call or sign it pre-transaction
   - `exclude`: infra, oracle, admin, keeper, implementation-only, standard token already covered
4. For every `cover`, snapshot verified ABI with provenance.
5. For every external `payable` or `nonpayable` function, triage `cover` or `exclude`.
6. Add EIP-712 signed structs under `signed_structs`.
7. Run surface gate.

Command:

```bash
cd registryV2
npm run check:surface
```

Protocol-agnostic red flags that must be `cover` unless a standard adapter explicitly owns them:

- `approve`
- `permit`
- `setApprovalForAll`
- `setAuthorization`
- `allow`
- `allowBySig`
- `delegate`
- `approveDelegation`
- `setOperator`
- `setRelayerApproval`
- any function that grants, revokes, moves, borrows, stakes, locks, unwraps, claims, bridges, signs, or delegates.

### P1. Function Mapping

For every COVER selector, decide the minimum tier.

| Question | Yes | No |
|---|---|---|
| Existing ActionBody can express this intent? | Tier 1 candidate | Tier 3 schema extension |
| Values can be mapped with `$args`, `$tx`, `$to`, `$chain`, static `$resolved`? | declarative manifest | Tier 2 generic engine extension |
| Field is user-legible as decoded? | no live field needed | add live input or documented defer |

Manifest strategy selection:

| shape | strategy |
|---|---|
| one function call -> one action | `single_emit` |
| router opcode stream | `opcode_stream_dispatch` |
| array elements -> repeated actions | `array_emit` |
| tagged bytes payload | `tagged_dispatch` |
| contract multicall bytes[] | `multicall_recurse` |
| EIP-712 signature | typed-data match + appropriate emit |

Required notes in every non-trivial manifest:

- primary source for address/ABI/selector
- why the ActionBody domain/action is semantically correct
- any skipped side effect
- any static `$resolved` or `$derived` assumption
- live input defer reason if `live_inputs` is empty but user readability is not obvious

### P2. Semantic Oracle Assignment

Every COVER selector gets at least one semantic oracle class.

| selector kind | required oracle |
|---|---|
| simple flat mapping | projection preferred |
| permission grant/revoke | `expect_body` or projection for authorizer/authorized/spender/flag |
| token/asset amount movement | projection for asset + amount + recipient/on_behalf_of |
| router/nested/multicall | curated corpus with `expect_body` for every meaningful child action |
| hash/ID derived field | field-level golden or projection with independent derivation |
| live input source | `expect_body` for source metadata/function name |
| unsupported/excluded | corpus `expect:error` or coverage exclude reason |

Semantic-critical fields by domain:

| domain | must pin |
|---|---|
| `token` | token address, amount/id, owner/from, recipient/to, spender/operator, approval flag |
| `permission` | authorizer, authorized/spender/operator, scope kind, grant/revoke boolean, protocol name |
| `amm` | token_in, token_out, amount_in/out/min/max, recipient, pool/venue, fee tier, path endpoints |
| `lending` | asset, collateral, debt asset, amount, borrower/on_behalf_of, delegatee, market/pool/comet |
| `liquid_staking` | staked/wrapped token, amount/shares, owner/recipient, withdrawal id, live conversion source |
| `staking` | staked token, amount, unlock time, gauge/validator, reward token, recipient |
| `airdrop` | token, claimant/recipient, amount/id/proof presence |
| `perp` | market, side, size, collateral, leverage/margin, recipient/account |
| `multicall` | child action count and semantic fields inside each meaningful child |
| `unknown` | reason: intentionally unsupported or non-user operation |

If a field appears in this table and no oracle pins it, the selector is `untested_semantic`.

### P3. Corpus and Projection Authoring

Corpus rules:

1. Keep only representative real txs and curated edge txs.
2. Do not dump raw 10k samples into git.
3. For every high-value selector, include at least:
   - one real tx if observed
   - one hand edge if permission/value-bearing
   - one failure/excluded example if intentionally unsupported
4. Add `expect_body` for semantic-critical fields.
5. `tx_hash` is preferred for real txs.
6. `value` must be decimal wei in committed corpus.
7. `v3-harness import-*` normalizes RPC proxy hex quantities before writing corpus JSON.

Projection rules:

1. One projection per selector shape, not per tx.
2. Keep projections independent from manifest emit.
3. Use raw ABI decode and simple derivation helpers only.
4. Projection failures are `mis_decoded`, not flaky tests.
5. If a selector has multiple modes, split by discriminant:
   - `swap_kind`
   - `command opcode`
   - `interestRateMode`
   - `operation kind`
   - EIP-712 `primaryType` / `witnessType`

### P4. Gap Loop

Every run emits gaps into the same vocabulary.

```text
uncovered          -> author manifest or mark EXCLUDE with reason
decode_error       -> fix abi_fragment, strategy, placeholder, or Tier2 builder
mis_decoded        -> fix emit mapping, resolver, derivation, or ActionBody schema
untested_semantic  -> add expect_body/projection/field-level golden
excluded           -> keep if reason still valid against primary source
```

No protocol moves to done while any COVER selector is `uncovered`, `decode_error`, `mis_decoded`, or `untested_semantic`.

### P5. Landing Gate

Minimum commands:

```bash
cd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2

cd registryV2
npm run build
npm run check:surface
npm run check:manifest
cd ..

cargo test -p policy-engine-integration-tests --test v3_decode_harness -- --nocapture
cargo test --workspace
```

If Tier 2/Tier 3/WASM-facing code changed:

```bash
./scripts/wasm-build.sh
```

Completion evidence must include:

- exact files added/changed
- gate output
- remaining WARNs, if any, explicitly scoped outside the protocol or justified
- any deferred selector/action with reason and issue/follow-up

---

## 4. Sub-Agent Instruction Templates

Use sub-agents for breadth, but make each prompt self-contained.

### P0 Contract Discovery Template

```text
Repo: /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2.
Task: Protocol <PROTOCOL> contract inventory for ScopeBall V3 onboarding.

Read:
- crates/integration-tests/PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md
- registryV2/surface/README.md

Find all user-facing contracts and EIP-712 signing surfaces for <PROTOCOL> on chains <CHAINS>.
Use only primary sources for final address claims: official deployments docs, official GitHub deployment artifacts, on-chain registry/address provider, verified explorer pages.
Use DefiLlama/Dune/Etherscan labels only as discovery challenges, not final proof.

Output artifacts:
- registryV2/surface/<protocol>/_deployments.json
- list of contracts requiring ABI snapshots
- unresolved candidates with why not verified

Do not author manifests. Do not touch unrelated files.
```

### P1 Selector Mapping Template

```text
Repo: /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2.
Task: Map <PROTOCOL> selector <SELECTOR> / signature <SIG> into V3 ActionBody.

Read:
- crates/integration-tests/PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md
- crates/integration-tests/ACTIONBODY_EXTENSION_GUIDE.md
- similar existing manifest: <PATH>

Decide Tier 1/2/3.
If Tier 1, author registryV2/manifests/<protocol>/<contract>/<function>@1.0.0.json.
If existing ActionBody is insufficient, propose exact Tier3 extension touchpoints.
If generic builder is insufficient, propose exact Tier2 extension point.

Add or specify semantic oracle:
- expect_body assertions or projection fields for all semantic-critical fields.

Run or report:
- cd registryV2 && npm run build && npm run check:manifest
```

### P2 Corpus/Oracle Template

```text
Repo: /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2.
Task: Build semantic corpus/projection for <PROTOCOL> selector <SELECTOR>.

Read:
- crates/integration-tests/PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md
- crates/integration-tests/README.md

Collect representative real txs from Etherscan/Dune if available.
Create/extend crates/integration-tests/data/golden/v3-decode/<protocol>/corpus.json.
For each pass entry, add expect_domain and expect_body for token/amount/recipient/spender/pool/fee/live-source fields.
If selector is simple enough, create projection spec instead of per-tx repetition.

Do not mark semantic-critical selector done without expect_body or projection.
```

---

## 5. Migration Policy for Existing Protocols

Do not flip global strict mode. Migrate protocol by protocol.

Recommended order:

1. Add `expect_body` engine as optional and keep all existing corpus green.
2. Add `expect_body` only to newly found regressions and high-risk selectors.
3. Add projections for simple/high-volume selectors.
4. Add semantic lints in warn mode.
5. Enable `audit --protocol <p> --strict` for one protocol.
6. Once a protocol has zero strict gaps, document it as strict-migrated.

Existing corpus without `expect_body` is legacy-valid but not semantically complete. Treat it as coverage evidence, not correctness evidence.

---

## 6. Definition of Done

A protocol is onboarded only when all of these are true:

- `_deployments.json` exists or omission is explicitly approved for that protocol.
- Every covered contract has ABI snapshot and coverage.
- `check:surface` has no failures for the protocol.
- Every COVER selector has manifest or documented Tier B/Tier 3 implementation.
- Every COVER selector has at least one semantic oracle:
  - projection, or
  - `expect_body`, or
  - field-level Rust golden for cases that cannot be represented yet.
- Every permission/value-bearing selector has a hand edge case.
- Every router/nested selector has at least one curated real or hand-built corpus with child-action assertions.
- `v3_decode_harness` passes.
- workspace tests pass for touched areas.
- Any WARN/defer has a reason and owner.

If any item is not proven by current files or command output, the protocol is not done.
