# ScopeBall Protocol-Agnostic Onboarding Framework

> 목적: 새 프로토콜마다 다시 생각하지 않도록, **프로토콜 독립 코드 골격과 실행 인스트럭션**을 고정한다. 이 문서는 구현 전체를 한 번에 끝내는 계획이 아니라, 어떤 프로토콜에도 반복 적용할 수 있는 framework contract 이다.
>
> 범위: V3 `ActionBody[]` 디코드 경로가 중심이다. 레거시 `ActionEnvelope` 경로는 고려하지 않는다.
> 단, 실제 product verdict path 는 decoded `ActionBody` 를 `lowering_v2` → per-policy schema/Cedar evaluation 으로 넘긴다. 새 domain/action/live field 를 추가하면 downstream policy contract 까지 같은 온보딩 범위로 본다.

---

## 0. 목표 모델

프로토콜 온보딩 완료는 “테스트가 green” 이 아니다. 완료는 아래 다섯 층이 모두 증명된 상태다.

| 층 | 질문 | 현재 근거 | 부족하면 |
|---|---|---|---|
| **Surface** | user-facing contract/function/signature 를 빠뜨리지 않았나? | `registryV2/surface/**` + `check:surface` | `_deployments.json`, ABI snapshot, coverage 보강 |
| **Shape** | manifest 가 실제 `ActionBody` 타입으로 round-trip 되나? | `v3-harness validate`, `oracle.rs` typed round-trip | manifest/Tier3 schema/lowering 수정 |
| **Semantic** | token, amount, recipient, spender, pool, path, live source 가 맞나? | `expect_body`, field-level golden, future projection | assertion/projection 추가 후 decoder 수정 |
| **Policy contract** | decoded action 이 runtime policy model 로 내려가나? | `lowering_v2`, `schema/per_policy.rs`, Cedar schema/action registration tests | lowering/schema/cedarschema 등록 |
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

현재 구현된 CLI 는 `fuzz`, `validate`, `coverage`, `replay`, `corpus`, `import-*` 이다. `projection.rs`, `semantic_lints.rs`, `audit.rs` 와 `audit --strict` 는 설계 목표이며 아직 실행 가능한 gate 로 취급하지 않는다. 현 landing 은 아래 §3/P4 의 manual gate 조합으로 수행한다.

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

Future target CLI, not implemented locally yet:

```bash
cargo run -p policy-engine-integration-tests --bin v3-harness -- audit \
  --protocol <protocol> \
  --strict
```

Current manual equivalent:

```bash
cd registryV2
npm run build
npm run check:surface
npm run check:manifest
cd ..
cargo run -p policy-engine-integration-tests --bin v3-harness -- coverage
cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations <N>
cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus
cargo test -p policy-engine-integration-tests --test v3_decode_harness
```

Audit stages:

1. build or verify `registryV2/index` freshness.
2. `check:surface --strict-protocol <protocol>`.
3. `validate --filter <protocol> --strategy all`.
4. protocol-filtered corpus with required `expect_body` / future projections.
5. semantic lints in strict mode once implemented.
6. coverage report: selectors with no real tx, no edge, no projection, no body assertions.
7. single JSON summary under `crates/integration-tests/logs/<protocol>/YYYY-MM-DD-audit.json`.

Audit bucket vocabulary:

| bucket | meaning | disposition |
|---|---|---|
| `correct` | routed + shape + semantic assertions/projection pass | done |
| `untested_semantic` | shape pass but no semantic oracle for critical fields | add `expect_body` or projection |
| `mis_decoded` | semantic assertion/projection fails | fix manifest/Tier2/Tier3 |
| `unknown_protocol_address` | known protocol selector hits an address outside the P0 universe/surface | reopen P0 universe, add cover/exclude/defer, then manifest/resolver if COVER |
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

### Worktree and Commit Discipline

Start every protocol onboarding in a dedicated branch inside the requested worktree. If the user provides an onboarding worktree/cwd, run `git switch -c feat/<protocol>-onboarding` there first, or switch to that branch if it already exists. If no worktree is provided, create one with `git worktree add -b feat/<protocol>-onboarding ../<dir> <base>`.

Commit at the end of each phase, or each smaller reviewable contract/function batch when a phase is large. Use explicit staging only (`git add <file>`), never blanket `git add -A`. Sub-agents and Claude Code produce candidate results; the main session owns verification and commits.

Run the onboarding in one continuous pass once branch setup and external data lanes are ready. Phase commits are checkpoints, not permission prompts; after a phase commit, continue into the next phase. Stop only for actions requiring explicit user approval, missing external authentication/data access, scope ambiguity that first-party sources cannot resolve, or a repeated hard blocker.

Do not merge the onboarding branch back into the base worktree automatically after the framework is complete. Merge only when the user explicitly requests it.

### Phase Exit Gates

Every phase is evidence-gated. Before saying a phase is complete, update `crates/integration-tests/onboarding/<protocol>/evidence.md` and make sure that phase's mandatory rows are `done` or concrete `blocked`.

| phase | cannot complete until |
|---|---|
| P0 research | Codex + Claude/sub-agent discovery, first-party disposition, pool/factory address universe disposition if applicable, token-surface inventory, surface artifact, and `check:surface` evidence are recorded |
| P1 authoring | every COVER selector has ActionBody/Tier3 mapping, red-flag review, manifest/Tier3 artifact list, live_field/enrichment decision, remote method disposition, and manifest validation evidence |
| P2 test corpus | synthetic fuzz/edge evidence and Etherscan+Dune real-tx evidence are recorded, or the exact external-data blocker is recorded |
| P3 develop | all P2 gaps are bucketed, every fix maps back to a gap/selector/tx/seed, reruns are recorded, and remaining gaps have disposition |
| P4 land | full landing gate outputs, staged file list, commit hash, remaining WARN/defer list, and no-auto-merge statement are recorded |

Before any phase-complete claim, run:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <protocol> --phase <p0|p1|p2|p3|p4|all>
```

If this fails for the phase being claimed, the phase is incomplete. `blocked` is allowed only with a concrete artifact and at least one concrete Blockers table row.

### Agent Orchestration

Protocol onboarding is too long for one linear context. Use sub-agents or Claude Code for independent work packets, but treat every result as untrusted until verified by the main session.

Default split:

- P0 contract discovery: Codex + Claude Code independent discovery, then union/diff/1st-source verification.
- P0 token inventory: separate token-surface pass using `crates/integration-tests/TOKEN_INVENTORY_GUIDE.md`.
- P1 selector authoring: selector batches, especially permission/fund-move selectors.
- P1 Tier3 design: new `ActionBody` / `lowering_v2` / `cedarschema` changes require independent review before landing.
- P2 synthetic: independent edge-case matrix and fuzz seed plan.
- P2 real tx: Etherscan/Dune pull + verdict bucketing by source.
- P3 gap triage: independent root-cause classification for hard failures.
- P4 audit: "what is still missing?" review against source docs and local gates.

Sub-agent prompt requirements:

- include repo path, branch/worktree, phase, exact scope, non-goals
- list required docs and exact files/symbols to inspect
- name existing implementations to mirror
- define output format and expected artifact paths
- include guardrails: first-party sources, no unrelated churn, no commits, uncertain items marked unverified

Merge rule:

- Main session compares Codex/sub-agent outputs, records disagreements, verifies accepted items against local code or first-party sources, then runs the relevant gate. No sub-agent output is copied blindly.

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
   - Codex current-session research + Claude Code headless research, candidate-only
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

Dual-agent rule:

1. Current Codex session performs P0 research from official/verified sources.
2. Claude Code receives the same discovery prompt via headless CLI, for example:
   ```bash
   claude -p "<P0 discovery prompt>" --add-dir /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2
   ```
3. Merge Codex ∪ Claude ∪ official deployment list ∪ secondary sweeps.
4. Any Codex-only or Claude-only candidate is high-priority for 1st-source verification.
5. LLM output never becomes ground truth. Only official deployment artifacts, verified ABI snapshots, and `check:surface` dispose candidates.

### P0.1. Pool / Factory Address Universe

Goal: pool-heavy protocols do not silently degrade into "representative pool"
onboarding.

This section is mandatory for protocols where users call many factory-created
or registry-listed child contracts directly, including Curve, Balancer,
Aerodrome, Uniswap V2-style pairs, lending vault factories, and similar
pool/vault universes.

Required artifacts:

- address-universe source: official pool list, factory event range, on-chain
  registry/address provider, verified deployment artifact, or Dune decoded
  namespace query.
- retrieval command/query and count.
- machine-readable artifact with a nonzero source count. A zero-count universe
  or zero-target tx pull is a filter/schema bug until proven otherwise; do not
  accept it as evidence.
- committed `registryV2/surface/<protocol>/_address_universe.json` or
  `_pool_universe.json`, validated with
  `npm run check:universe -- --protocol <protocol>`.
- disposition table: every candidate address is `cover`, `exclude`, or `defer`
  with reason.
- if only a subset is concretely covered, a batch boundary plus explicit
  decision: manual concrete manifests now, protocol source resolver/generator
  now, or deferred resolver/generator follow-up.

Rules:

- Do not let "covered pool" define the universe. Build the candidate universe
  first, then decide cover/exclude/defer.
- For exact callkey registries, every missing child address is a production miss
  even when the selector and ABI are already supported.
- If the protocol has thousands of pools, batching is allowed only with an
  explicit boundary such as chain, factory, creation block range, official list
  page, TVL cutoff, or product family. "Top pools" without a reproducible source
  and deferred remainder is not a complete P0.
- If a generated/source resolver can safely enumerate the universe, prefer it
  over hand-maintaining large `chain_to_addresses` arrays. If not implemented,
  record why manual concrete coverage is acceptable for the batch.
- If pool-specific metadata is required to emit the correct `ActionBody`
  (coin map, LP token, gauge, vault asset, market id), do not use an
  address-only resolver. Add a materialized protocol source instead:
  resolver returns per-address context, manifest uses exact `$source.*`
  placeholders, build-index emits one concrete bundle per address, and P4
  `check:universe --require-cover-linkage` proves every `cover` address has at
  least one generated callkey.
- Unknown to-addresses observed later with known protocol selectors are P0/P2
  hard gaps, not ordinary low-traffic misses.

### P0.5. Token Inventory

Goal: no protocol-issued or protocol-critical token is invisible to ERC standard auto-enumeration.

Required artifacts:

- `registryV2/tokens/<chainId>/<lowercase-address>.json` for every in-scope token.
- P0 log note for large protocols that intentionally batch long-tail pools/tokens.

Use `crates/integration-tests/TOKEN_INVENTORY_GUIDE.md` as the source instruction. Token inventory is required when the protocol has any of:

- fungible LP/pool share tokens (Curve, Balancer, Aerodrome, Uniswap V2)
- receipt/share tokens (Compound cTokens, Aave aTokens, ERC4626 vault shares)
- debt tokens
- governance/base tokens directly used by protocol flows
- pool underlyings referenced by ActionBody fields or token_kind metadata
- NFT position-manager collections (collection-level only; never enumerate token_id instances)

Rules:

- Token JSON is the input for `tokens:erc20` / `tokens:erc721` / `tokens:erc1155` manifests. Missing token JSON means standard ERC calls to that address may have no callkey.
- Research token metadata from static first-party sources: official token list, official pool list, official address-book, verified explorer token page. Do not use ad-hoc RPC reads for symbol/decimals.
- Register underlyings recursively when a `token_kind` references them.
- For Curve-like protocols, covered pools require their LP token and underlying tokens. Long-tail pools can be deferred only with an explicit batch boundary in the P0 log.
- After token edits, run `cd registryV2 && npm run build` or `npx tsx scripts/build-index.ts`.

Action-model preflight:

- `crates/simulation/reducer/src/action` is an intent catalog, not a protocol list.
- If a protocol maps cleanly to existing domains/actions, no Tier 3 work is needed.
- If a COVER selector has user-risk semantics that no existing action can express, add/extend Tier 3 `ActionBody` before authoring manifests.
- Permission grants/revokes are never hidden behind `Unknown`; add a dedicated action when needed.

Tier 3 deliverables are not only Rust `ActionBody` structs. A new action is complete only when all downstream contracts exist:

- `crates/simulation/reducer/src/action/<domain>/**` — protocol-agnostic intent schema.
- reducer/effect/view/sync touchpoints — exhaustive state and action walking still compile and preserve semantics.
- `crates/policy-engine/src/lowering_v2/<domain>/<action>.rs` — converts `ActionBody` into Cedar request context.
- `schema/policy-schema/actions/<domain>/<action>.cedarschema` — policy-visible context/action declaration.
- `crates/policy-engine/src/schema/{mod.rs,action_name.rs,per_policy.rs}` — manual schema registration and resolver table entry.
- leaf lowering conformance test — strict-validates `lower_action` output against `compose_per_policy`.
- manifest + corpus/golden — proves actual calldata/typed-data reaches the new action shape.

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
- required remote policy-RPC/live/enrichment methods, with one disposition:
  local handler exists, configured endpoint was tested, or explicit blocker.
  The old in-repo Node `policy-rpc/` service no longer exists, so catalog
  presence alone is not evidence that a remote method will work.

### P2. Semantic Oracle Assignment

Every COVER selector gets at least one semantic oracle class.

| selector kind | required oracle |
|---|---|
| simple flat mapping | `expect_body` now; projection preferred after executor lands |
| permission grant/revoke | `expect_body` now; future projection for authorizer/authorized/spender/flag |
| token/asset amount movement | `expect_body` now; future projection for asset + amount + recipient/on_behalf_of |
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

Synthetic test floor:

1. Run full-surface fuzz with a fixed seed and JSON log.
   ```bash
   target/debug/v3-harness coverage
   target/debug/v3-harness fuzz --iterations 5000 --seed 0x5C09EBA1 \
     --json crates/integration-tests/logs/<protocol>/YYYY-MM-DD-synthetic.json
   ```
2. Replay every hard failure by callkey + seed before editing.
3. Add hand edges for every permission/value-bearing/nested/array/opcode/typed-data selector.
4. Edge menu: zero/one/max amount, finite/max/revoke permission, recipient sender/third-party, empty/singleton/multi arrays, malformed/truncated calldata, unsupported opcode, malformed path bytes, nested supported+unsupported child mix.
5. Edge `pass` entries must pin semantic-critical fields with `expect_body`; edge `error` entries must pin `expect_error`.

Real-tx floor:

1. Etherscan API/MCP is the bulk lane. One `txlist` API call currently can return up to 10,000 tx, so the default target is **10,000 tx/protocol**, not 10,000 API calls. Re-check the current Etherscan docs before each onboarding; Free tier record limits are scheduled to drop to 1,000/request on 2026-07-01.
2. Use `.env` `ETHERSCAN_API_KEY`; daily 100,000-call capacity is a safety budget, not a spending target.
3. Fetch adapter-blind by P0 cover addresses, stratified by selector and block range. Do not choose txs by existing manifests.
   For pool-heavy/factory protocols, also sweep the P0 candidate/universe
   addresses or run selector/address stats over the whole universe. Do not limit
   real-tx sampling to the subset already selected for concrete manifests.
4. Every COVER selector should have real tx sample >= 1, or an explicit low-traffic/absent note.
5. If a tx uses a known protocol selector against an address absent from the
   P0 universe/surface/registry, bucket it as `unknown_protocol_address` and
   return to P0/P1. Do not hide it as a generic `no_declarative_v3_mapper`.
6. Dune MCP/API is the gap lane for Free-tier Etherscan txlist gaps such as Base/OP, decoded namespaces, selector stats, and cross-chain joins. Before relying on it, run MCP calibration: usage baseline, LIMIT 100/1000/5000 probe, partition WHERE, credit delta log.
7. If Etherscan or Dune is unavailable, do not mark P2 real-tx complete. Record `blocked_external_data`, completed synthetic/golden scope, and the addresses/selectors to replay once the tool is connected.
8. Commit only dedup representative corpus/golden entries; keep raw 10k+ dumps out of git.

Tool connection hints: Etherscan remote MCP is `https://mcp.etherscan.io/mcp` with bearer-token auth from `ETHERSCAN_API_KEY`. Dune remote MCP is `https://api.dune.com/mcp/v1` with OAuth or API-key auth. Never commit keys or raw external dumps.

### P3. Develop: Corpus, Projection, and Gap Loop

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

Gap loop rules:

Every run emits gaps into the same vocabulary.

```text
uncovered          -> author manifest or mark EXCLUDE with reason
decode_error       -> fix abi_fragment, strategy, placeholder, or Tier2 builder
mis_decoded        -> fix emit mapping, resolver, derivation, or ActionBody schema
untested_semantic  -> add expect_body/projection/field-level golden
unknown_protocol_address -> reopen P0 universe and cover/exclude/defer address
excluded           -> keep if reason still valid against primary source
```

No protocol moves to done while any COVER selector is `uncovered`, `decode_error`, `mis_decoded`, `untested_semantic`, or `unknown_protocol_address`.

### P4. Landing Gate

Minimum commands:

```bash
cd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2

cd registryV2
npm run build
npm run check:surface
npm run check:universe -- --protocol <protocol> --require-cover-linkage
npm run check:manifest
cd ..

cd browser-extension
node .yarn/releases/yarn-4.14.1.cjs vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts
cd ..

cargo run -p policy-engine-integration-tests --bin v3-harness -- coverage
cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations <N>
cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus
cargo test -p policy-engine-integration-tests --test v3_decode_harness -- --nocapture
cargo test --workspace
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <protocol> --phase all
```

If Tier 2/Tier 3/WASM-facing code changed:

```bash
./scripts/wasm-build.sh
```

Current product-path caveats to check explicitly:

- EIP-712 typed-data manifests can pass registry/WASM/harness tests while browser orchestration may still mark typed signatures as not routed. Treat typed-data support as harness-proven, product-unproven unless the extension route is exercised.
- Native-transfer sentinel `0x00000000` is valid in WASM/harness corpus, but selector-less calldata can miss earlier in the extension TypeScript route. Do not claim production native-transfer support without checking that boundary.
- Required remote policy-RPC/live/enrichment methods fail closed when no
  endpoint supplies them. The standalone Node `policy-rpc/` package has been
  removed from this worktree; do not claim those methods are production-ready
  unless a local handler or configured endpoint was tested.

Completion evidence must include:

- `crates/integration-tests/onboarding/<protocol>/evidence.md`, copied from `ONBOARDING_EVIDENCE_TEMPLATE.md`
- every P0/P1/P2/P3/P4 mandatory row marked `done` or concrete `blocked`; otherwise do not claim the phase is complete
- exact files added/changed
- gate output
- remaining WARNs, if any, explicitly scoped outside the protocol or justified
- any deferred selector/action with reason and issue/follow-up
- P0 Claude Code/sub-agent command or agent id, output summary, union/diff disposition, first-party verification result, and pool/factory address-universe disposition if applicable
- P1 authoring evidence: per-COVER selector ActionBody/Tier3 mapping, permission/fund-movement red-flag review, manifest file list, live_field/enrichment decision, required remote method disposition, Tier3 downstream artifact list if applicable, `check:manifest` output
- P2 synthetic evidence: fuzz seed/iteration command, fixed edge matrix, pass/error corpus disposition
- P2 Etherscan evidence: txlist command/query, api call count, raw tx count, unique selector count, per-COVER-selector real tx coverage, pool/factory candidate-universe sweep if applicable
- P2 external tx-pull evidence: nonzero target address count, or concrete blocker explaining why the target set cannot be built yet
- P2 Dune evidence: usage baseline, query id/SQL summary with partition WHERE, rows returned, credit cost or usage delta, selected tx hashes, selector/address stats for pool-heavy gaps if applicable
- P3 develop evidence: gap buckets, fix-to-gap mapping, rerun output, corpus `expect` flips/exclusions, remaining defer/blocker disposition
- P4 land evidence: `registryV2 npm run build`, build-index vitest, `check:manifest`, `check:surface`, v3-harness coverage/fuzz/corpus, workspace test output, wasm/fmt/clippy/typecheck outputs where applicable, staged file list, commit hash
- check-onboarding-evidence `--phase all` pass output
- explicit statement that no base/worktree merge was performed unless the user requested it
- explicit `blocked_external_data` entry if Etherscan/Dune/Claude Code could not be used

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
- expect_body assertions or field-level Rust golden for all semantic-critical fields.
- Projection fields are acceptable only after the projection executor exists.

Run or report:
- cd registryV2 && npm run build && npm run check:manifest
```

### P2 Corpus/Oracle Template

```text
Repo: /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2.
Task: Build semantic corpus / field oracle for <PROTOCOL> selector <SELECTOR>.

Read:
- crates/integration-tests/PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md
- crates/integration-tests/README.md

Collect representative real txs from Etherscan and run Dune calibration/pinpoint where applicable. If either source is unavailable, write `blocked_external_data` in `crates/integration-tests/onboarding/<protocol>/evidence.md`; do not silently skip it.
Create/extend crates/integration-tests/data/golden/v3-decode/<protocol>/corpus.json.
For each pass entry, add expect_domain and expect_body for token/amount/recipient/spender/pool/fee/live-source fields.
If selector is simple enough, note future projection spec candidates, but do not treat them as executable until projection support exists.

Do not mark semantic-critical selector done without expect_body or field-level golden. Projection can replace this only after implementation.
```

---

## 5. Migration Policy for Existing Protocols

Do not flip global strict mode. Migrate protocol by protocol.

Recommended order:

1. Add `expect_body` engine as optional and keep all existing corpus green.
2. Add `expect_body` only to newly found regressions and high-risk selectors.
3. Add projections for simple/high-volume selectors once projection support exists.
4. Add semantic lints in warn mode once lint support exists.
5. Enable `audit --protocol <p> --strict` for one protocol once the audit CLI exists.
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
  - projection when implemented, or
  - `expect_body`, or
  - field-level Rust golden for cases that cannot be represented yet.
- Every permission/value-bearing selector has a hand edge case.
- Every router/nested selector has at least one curated real or hand-built corpus with child-action assertions.
- `v3_decode_harness` passes.
- workspace tests pass for touched areas.
- Any WARN/defer has a reason and owner.

If any item is not proven by current files or command output, the protocol is not done.
