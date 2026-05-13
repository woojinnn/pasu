# Action 재정의 및 Adapter Modular 리팩토링 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **각 Task 는 default 로 fresh `codex:codex-rescue` 서브에이전트에 위임.** Claude (orchestrator) 는 Codex 결과를 `comprehensive-review:code-reviewer` 로 review 한 뒤 사용자 승인까지 받고 다음 Task 진행.

**Goal:** policy-engine 의 5-variant `Action` 을 schema/ JSON 정의에 맞춘 32-variant 통합 타입으로 재정의하고, `crates/adapters/` 아래에 `abi-resolver`(Decoder) / `mappers`(Mapper) / `call-adapter`(CallAdapter composite) / `sign-resolver`(SignAdapter composite) / `request-router`(dispatcher) 5개 sub-crate 로 modular 화한다. 정책 평가는 본 PR 에서 Swap-only.

**Architecture:** Decoder 와 Mapper 는 내부 building block; CallAdapter 와 SignAdapter 는 composite (둘 다 `build()` → `Vec<ActionEnvelope>`). request-router 는 두 composite trait 만 알고 RPC method type 으로 dispatch. Registry 는 모두 trait + 1개 in-memory 구현체 (향후 remote-fetch registry 도입에 호환). policy-engine 은 ActionEnvelope → Verdict 만 담당.

**Tech Stack:** Rust 1.x stable, Cargo workspace, serde, alloy/ethers-rs (calldata 디코딩), Cedar (policy DSL), tokio (async I/O — sign-resolver/abi-resolver), wasm-bindgen (extension WASM), axum (web-server), TypeScript (browser extension).

**Spec:** `docs/specs/2026-05-13-action-and-adapter-refactor-design.md`

---

## 사용 규칙 (Plan executor 가 매 task 시작 전 읽기)

1. **위임 원칙**: 각 Task 의 "Codex Delegation" 블록을 그대로 `codex:codex-rescue` 서브에이전트에 전달. Codex 가 TDD (failing test → impl → passing test) 로 진행하도록 prompt 에 명시되어 있음.
2. **검증**: Codex 가 완료 후 orchestrator 가 "Verification" 블록의 명령을 직접 실행. 실패 시 Codex 에 동일 task 로 재위임 (max 3회).
3. **코드 리뷰**: Verification 통과 후 "Review" 블록의 `comprehensive-review:code-reviewer` agent 호출. 리뷰 결과를 사용자에게 제시 → 사용자 승인 후 commit.
4. **커밋 메시지**: 각 Task 의 "Commit" 블록 메시지 사용 (`Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` 포함).
5. **Phase 경계**: 한 Phase 의 모든 Task 완료 후 Phase-end Review Gate (각 Phase 마지막 섹션) 통과해야 다음 Phase 진행.
6. **PR 단위**: 각 Phase = 별도 PR. Phase 종료 시 `feature/phase-N-<short>` 브랜치 → main 으로 PR. CI green 필수.
7. **branch 운영**: 작업은 `feature/action-adapter-refactor` 베이스 브랜치에서 분기한 Phase 브랜치 (`phase-1-action-types`, `phase-1_5-relocate-crates`, ...).

---

## Glossary

| 용어 | 의미 |
|---|---|
| `Action` | 새 32-variant enum, `crates/policy-engine/src/action/` 에 정의. schema/ JSON 의 각 action 파일 = enum variant 1개. |
| `ActionEnvelope` | `{ category, action, fields }` 컨테이너. JSON 직렬화 시 schema/root.json 의 envelope 모양과 일치. |
| `RootRequest` | top-level wrapper (`schemaVersion`, `chainId`, `from`, `to`, `value`, `selector`, `actions: Vec<ActionEnvelope>`, ...). |
| `LegacyAction` | 본 PR 동안 임시 유지되는 기존 5-variant Action. Phase 6 에서 제거. |
| `Decoder` | `abi-resolver` 의 trait. `(chain_id, to, selector)` → `DecodedCall`. |
| `Mapper` | `mappers` 의 trait. `DecodedCall` → `Vec<ActionEnvelope>`. |
| `CallAdapter` | `call-adapter` 의 composite trait. `build()` 내부에서 Decoder→Mapper 호출. |
| `SignAdapter` | `sign-resolver` 의 composite trait. `build()` 직접 SignRequest → `Vec<ActionEnvelope>`. |
| `MatchKey` | Registry lookup 키. `CallMatchKey { chain_id, to, selector }`, `SignMatchKey { chain_id, verifying_contract, primary_type }`. 모두 `Serialize + Deserialize`. |
| `Registry` | trait. 본 PR 의 구현체는 `InMemory*Registry` 하나. |

---

## Pre-flight (한번만)

### Task P.1: 작업 브랜치 생성 + Codex 셋업 확인

**Files:** 없음 (브랜치 생성만)

- [ ] **Step 1:** main 최신 동기화

```bash
git fetch origin
git checkout main
git pull --ff-only origin main
```

- [ ] **Step 2:** 베이스 브랜치 생성

```bash
git checkout -b feature/action-adapter-refactor
git push -u origin feature/action-adapter-refactor
```

- [ ] **Step 3:** Codex CLI 셋업 확인

`/codex:setup` 슬래시 명령 실행. 출력 OK 확인. ready 아니면 사용자에게 알리고 멈춤.

- [ ] **Step 4:** baseline build 확인 (이후 회귀 비교 기준)

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

세 명령 모두 통과 확인. 실패하면 main 자체가 broken — 사용자에게 알리고 멈춤.

---

# Phase 0 — Baseline Snapshot

**목적:** 현재 시스템의 wire-format 출력을 sample input 10종에 대해 캡쳐. 이후 Phase 들이 의도하지 않은 변화를 일으키지 않았는지 회귀 비교 기준.

**위임 대상:** Claude orchestrator (사용자 가이드 하에 직접 실행). Codex 불필요.

### Task 0.1: Golden Vector 입력 데이터셋 작성

**Files:**
- Create: `crates/integration-tests/data/golden/inputs/swap_uniswap_v2_exact_in.json`
- Create: `crates/integration-tests/data/golden/inputs/swap_uniswap_v2_exact_out.json`
- Create: `crates/integration-tests/data/golden/inputs/swap_uniswap_v3_exact_input_single.json`
- Create: `crates/integration-tests/data/golden/inputs/swap_uniswap_v3_exact_input_multi.json`
- Create: `crates/integration-tests/data/golden/inputs/swap_universal_router.json`
- Create: `crates/integration-tests/data/golden/inputs/permit2_permit_single.json`
- Create: `crates/integration-tests/data/golden/inputs/permit2_permit_batch.json`
- Create: `crates/integration-tests/data/golden/inputs/eip2612_permit.json`
- Create: `crates/integration-tests/data/golden/inputs/erc20_approve.json`
- Create: `crates/integration-tests/data/golden/inputs/unknown_selector.json`

- [ ] **Step 1:** 각 파일에 raw RPC payload JSON 작성. 예시:

```json
// swap_uniswap_v2_exact_in.json
{
  "label": "uniswap-v2-swapExactTokensForTokens",
  "rpc": {
    "method": "eth_sendTransaction",
    "params": [{
      "from": "0xa929022c9107643515f5c777ce9a910f0d1e490c",
      "to": "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
      "value": "0x0",
      "data": "0x38ed1739..."
    }]
  },
  "chain_id": 1
}
```

10개 파일 모두 작성. 실제 calldata 는 `crates/integration-tests/tests/fixtures/` 또는 `crates/adapters/uniswap-v2/tests/` 의 기존 fixture 에서 추출.

- [ ] **Step 2:** 커밋

```bash
git add crates/integration-tests/data/golden/inputs/
git commit -m "$(cat <<'EOF'
test: add Phase 0 golden vector input fixtures

10 sample RPC payloads covering V2/V3/UR swap, Permit2 single/batch,
EIP-2612 permit, ERC-20 approve, and unknown-selector. Used as
regression baseline across the action-adapter refactor.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 0.2: Pre-refactor 출력 캡쳐 스크립트

**Files:**
- Create: `scripts/capture_baseline.sh`
- Create: `crates/integration-tests/data/golden/baseline_pre_refactor/` (디렉토리)

- [ ] **Step 1:** 캡쳐 스크립트 작성

```bash
#!/usr/bin/env bash
# scripts/capture_baseline.sh
# Runs current main-branch adapters on golden inputs and dumps outputs.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INPUTS="$ROOT/crates/integration-tests/data/golden/inputs"
OUTDIR="$ROOT/crates/integration-tests/data/golden/baseline_pre_refactor"
mkdir -p "$OUTDIR"

for f in "$INPUTS"/*.json; do
  name=$(basename "$f" .json)
  echo "Capturing $name..."
  cargo run --quiet -p integration-tests --bin capture_baseline -- \
    --input "$f" --output "$OUTDIR/$name.json"
done

echo "Captured $(ls "$OUTDIR" | wc -l) baseline outputs."
```

- [ ] **Step 2:** capture_baseline 바이너리 추가

`crates/integration-tests/src/bin/capture_baseline.rs` 작성:

```rust
//! Capture current adapter pipeline output for a sample RPC payload.
//! Used only for Phase 0 baseline snapshot. Will be removed after the refactor.

use std::fs;
use std::path::PathBuf;
use clap::Parser;
use serde_json::Value;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let input: Value = serde_json::from_str(&fs::read_to_string(&args.input)?)?;
    let chain_id = input["chain_id"].as_u64().unwrap_or(1);
    let rpc_method = input["rpc"]["method"].as_str().expect("rpc.method");
    let rpc_params = &input["rpc"]["params"];

    let output = request_router::route_request(rpc_method, rpc_params.clone(), chain_id)?;
    fs::write(&args.output, serde_json::to_string_pretty(&output)?)?;
    Ok(())
}
```

`crates/integration-tests/Cargo.toml` 에 binary entry 추가:

```toml
[[bin]]
name = "capture_baseline"
path = "src/bin/capture_baseline.rs"
required-features = []
```

- [ ] **Step 3:** 스크립트 실행 + 결과 dump

```bash
chmod +x scripts/capture_baseline.sh
./scripts/capture_baseline.sh
ls crates/integration-tests/data/golden/baseline_pre_refactor/
# 10 .json files expected
```

- [ ] **Step 4:** 커밋

```bash
git add scripts/capture_baseline.sh crates/integration-tests/src/bin/capture_baseline.rs crates/integration-tests/Cargo.toml crates/integration-tests/data/golden/baseline_pre_refactor/
git commit -m "$(cat <<'EOF'
test: capture pre-refactor baseline outputs (Phase 0)

Adds capture_baseline binary + script and commits the captured JSON
outputs for the 10 golden input fixtures. Phases 1-6 must produce
JSON-equivalent output (modulo host:registry fields). Phases 7-8
may break compatibility atomically with TS update.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Phase 0 Review Gate

- [ ] 10개 baseline JSON 파일이 `baseline_pre_refactor/` 에 존재
- [ ] 각 파일이 유효한 JSON (`jq '.' file.json` 통과)
- [ ] 사용자에게 baseline 디렉토리 ls 결과 보여주고 명시 승인 받음
- [ ] PR 생성: `gh pr create --base main --head feature/action-adapter-refactor --title "[Phase 0] Baseline golden vectors" --draft`

---

# Phase 1 — Action 통합 타입 도입

**목적:** 32-variant `Action` enum + 공용 primitives + `ActionEnvelope` + `RootRequest` 를 `crates/policy-engine/src/action/` 에 추가. 기존 `Action` 은 `LegacyAction` 으로 임시 이름변경하여 빌드 유지.

**위임 대상:** Codex (`codex:codex-rescue`) 각 task 별.

**브랜치:** `phase-1-action-types` (feature/action-adapter-refactor 에서 분기)

### Task 1.1: 공용 primitives 모듈 (`action/common.rs`)

**Files:**
- Create: `crates/policy-engine/src/action/common.rs`
- Create: `crates/policy-engine/src/action/mod.rs`
- Modify: `crates/policy-engine/src/lib.rs` (action 모듈 노출)
- Test: `crates/policy-engine/src/action/common.rs` (inline `#[cfg(test)]`)

- [ ] **Codex Delegation**

```
TASK: Add common primitives module for the new 32-variant Action redesign.

Repo: /Users/woojin/Desktop/upside_academy/project/policy-engine
Branch: phase-1-action-types
Files to create:
  - crates/policy-engine/src/action/common.rs
  - crates/policy-engine/src/action/mod.rs

Files to modify:
  - crates/policy-engine/src/lib.rs: add `pub mod action;`

Required types (mirror schema/schema/common/_common.json):
  - `Address` (newtype, lowercase 0x + 40 hex chars, `FromStr` validates)
  - `Hex` (newtype, 0x + even hex chars)
  - `DecimalString` (newtype, base-10 u256-fitting digits only)
  - `AssetKind` enum: Native, Erc20, Erc721, Erc1155, Unknown (serde rename_all = "snake_case")
  - `AssetRef` struct: kind, chain_id, address?: Address, symbol?: String, decimals?: u8 (serde rename_all = "camelCase")
  - `AmountKind` enum: Exact, Min, Max, Unlimited, Estimated, Unknown
  - `AmountConstraint` struct: kind, value?: DecimalString
  - `ValiditySource` enum: TxDeadline, SignatureDeadline, GrantExpiration (serde rename_all = "kebab-case")
  - `Validity` struct: expires_at: DecimalString, source: ValiditySource (rename_all = "camelCase")
  - `UsdValuation` struct: value: String, as_of_ts?: u64, sources?: Vec<String>, stale_sec?: u64

Constraints:
  - All structs: derive Debug, Clone, PartialEq, Eq, Serialize, Deserialize
  - `Address::from_str` must reject non-hex / wrong length; lowercase normalize
  - serde rename_all matches schema JSON exactly (verify against schema/schema/common/_common.json)

Tests (TDD - write tests FIRST, see them fail, then implement):
  - test_address_normalize_uppercase_to_lowercase
  - test_address_reject_wrong_length
  - test_address_reject_non_hex
  - test_decimal_string_reject_non_digits
  - test_decimal_string_accept_max_u256
  - test_asset_ref_serde_roundtrip_erc20
  - test_asset_ref_serde_omit_optional_fields
  - test_amount_constraint_serde_roundtrip_all_kinds
  - test_validity_serde_kebab_case_source

Run:
  cargo test -p policy-engine --lib action::common
  cargo clippy -p policy-engine -- -D warnings

Approach:
  1. Write tests first in module's #[cfg(test)] mod tests block
  2. Run, verify failure
  3. Implement minimal types
  4. Run, verify pass
  5. Commit with message:
     "feat(policy-engine): add Action common primitives (Phase 1.1)"
```

- [ ] **Verification**

```bash
cargo test -p policy-engine --lib action::common -- --nocapture
cargo clippy -p policy-engine -- -D warnings
cargo build --workspace
```

Expected: 모든 9개 test 통과, clippy 무경고, workspace 빌드 OK.

- [ ] **Review**

```
Dispatch comprehensive-review:code-reviewer agent with:
"Review the diff for Task 1.1 (Action common primitives). Focus on:
1. Are newtype wrappers (Address, DecimalString, Hex) using proper validation?
2. Are serde attributes consistent with schema/schema/common/_common.json?
3. Are PartialEq/Eq/Hash derives appropriate? (Address should impl Hash.)
4. Any clippy lint suppressions and are they justified?
5. Are tests covering the edge cases (empty strings, max values, unicode)?
Return: APPROVE / REQUEST_CHANGES with specific line refs."
```

- [ ] **Commit**

Codex 가 자동 커밋. orchestrator 가 메시지 확인 후 push.

### Task 1.2: Category + ActionKind enum

**Files:**
- Create: `crates/policy-engine/src/action/envelope.rs`
- Modify: `crates/policy-engine/src/action/mod.rs` (`pub mod envelope;` + re-exports)

- [ ] **Codex Delegation**

```
TASK: Add Category enum and ActionEnvelope wrapper.

Repo: /Users/woojin/Desktop/upside_academy/project/policy-engine
Branch: phase-1-action-types

Files to create:
  - crates/policy-engine/src/action/envelope.rs

Files to modify:
  - crates/policy-engine/src/action/mod.rs: add `pub mod envelope;` and re-export `Category`, `ActionEnvelope`

Required types (mirror schema/schema/root.json):
  - `Category` enum (8 variants): Dex, Lending, Rwa, LiquidStaking, Restaking, Yield, Misc, Unknown (serde rename_all = "snake_case")

  - `ActionEnvelope` struct:
    ```rust
    pub struct ActionEnvelope {
        pub category: Category,
        #[serde(flatten)]
        pub action: Action,  // Action enum defined in Task 1.3+, use placeholder for now
    }
    ```

    For this task: define `ActionEnvelope` with a placeholder `Action` (unit enum with one Stub variant). Real Action variants land in Tasks 1.3-1.7.

  ```rust
  // Temporary placeholder until Tasks 1.3-1.7:
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(tag = "action", content = "fields", rename_all = "snake_case")]
  pub enum Action {
      Stub,  // remove in Task 1.7
  }
  ```

Tests:
  - test_category_serde_snake_case (each variant round-trips its JSON string)
  - test_action_envelope_json_shape_flat_action_field (verify { "category": "...", "action": "...", "fields": {...} } shape)

Run:
  cargo test -p policy-engine --lib action::envelope
  cargo clippy -p policy-engine -- -D warnings

Commit message: "feat(policy-engine): add Category and ActionEnvelope (Phase 1.2)"
```

- [ ] **Verification:** `cargo test -p policy-engine --lib action::envelope`, clippy clean.

- [ ] **Review:** comprehensive-review:code-reviewer on diff. Focus: serde flatten + tag/content interplay; JSON shape vs schema/root.json ActionEnvelope.

- [ ] **Commit:** Codex auto. Orchestrator verify + push.

### Task 1.3: Dex action structs (7 variants)

**Files:**
- Create: `crates/policy-engine/src/action/dex.rs`
- Modify: `crates/policy-engine/src/action/mod.rs` (add `pub mod dex;`)

- [ ] **Codex Delegation**

```
TASK: Implement 7 DEX action variants (Swap, AddLiquidity, RemoveLiquidity, MintLiquidityNft, BurnLiquidityNft, IncreaseLiquidity, DecreaseLiquidity).

Repo: /Users/woojin/Desktop/upside_academy/project/policy-engine
Branch: phase-1-action-types

Files to create:
  - crates/policy-engine/src/action/dex.rs

Files to modify:
  - crates/policy-engine/src/action/mod.rs: add `pub mod dex;`

Reference: schema/schema/actions/dex/*.json (7 files). Each JSON file's fields, types, and required/optional status must match the Rust struct exactly.

Required types (each derives Debug, Clone, PartialEq, Eq, Serialize, Deserialize, serde rename_all = "camelCase"):

1. SwapMode enum: ExactIn, ExactOut, Market, Unknown (rename_all = "snake_case")

2. SwapAction:
   - mode: SwapMode
   - token_in: AssetRef
   - token_out: AssetRef
   - amount_in: AmountConstraint
   - amount_out: AmountConstraint
   - recipient: Address
   - slippage_bps: Option<u32>
   - validity: Option<Validity>
   - fee_bps: Option<u32>
   - enrichment: SwapEnrichment (default empty)

3. SwapEnrichment (Default + serde with skip_serializing_if = "SwapEnrichment::is_empty"):
   - value_in_usd: Option<UsdValuation>
   - min_value_out_usd: Option<UsdValuation>
   - expected_value_out_usd: Option<UsdValuation>
   - allowance_covers_input: Option<bool>
   - input_fraction_of_portfolio_bps: Option<u32>
   - fn is_empty(&self) -> bool

4. AddLiquidityAction (per schema/schema/actions/dex/add_liquidity.json):
   - pool: PoolRef  // { address?: Address, id?: String, label?: String }
   - tokens: Vec<AssetRef>
   - amounts: Vec<AmountConstraint>
   - min_amounts_in: Option<Vec<AmountConstraint>>
   - lp_token: AssetRef
   - lp_amount: Option<AmountConstraint>
   - recipient: Address
   - validity: Option<Validity>

5. RemoveLiquidityAction (mirror remove_liquidity.json):
   - pool: PoolRef
   - lp_token: AssetRef
   - lp_amount: AmountConstraint
   - tokens: Vec<AssetRef>
   - min_amounts_out: Vec<AmountConstraint>
   - recipient: Address
   - validity: Option<Validity>

6. MintLiquidityNftAction (mirror mint_liquidity_nft.json):
   - pool: PoolRef
   - fee_bps: u32
   - tick_range: TickRange  // { lower: i32, upper: i32 }
   - tokens: [AssetRef; 2]
   - amounts: [AmountConstraint; 2]
   - min_amounts_in: [AmountConstraint; 2]
   - nft: AssetRef
   - recipient: Address
   - validity: Option<Validity>

7. BurnLiquidityNftAction (mirror burn_liquidity_nft.json):
   - pool: PoolRef
   - nft: AssetRef
   - token_id: DecimalString
   - tokens: [AssetRef; 2]
   - min_amounts_out: [AmountConstraint; 2]
   - recipient: Address

8. IncreaseLiquidityAction:
   - nft: AssetRef
   - token_id: DecimalString
   - amounts: [AmountConstraint; 2]
   - min_amounts_in: [AmountConstraint; 2]
   - validity: Option<Validity>

9. DecreaseLiquidityAction:
   - nft: AssetRef
   - token_id: DecimalString
   - liquidity: AmountConstraint
   - min_amounts_out: [AmountConstraint; 2]
   - recipient: Address
   - validity: Option<Validity>

Also: PoolRef and TickRange supporting structs in this file.

Tests:
  - For each of 7 actions: test_<action>_serde_roundtrip_minimal (required fields only)
  - For each of 7 actions: test_<action>_serde_roundtrip_full (all optional fields populated)
  - test_swap_enrichment_omitted_when_empty
  - test_swap_serde_matches_schema_fixture (load schema/schema/actions/dex/swap.json example, ensure compatible)

Run:
  cargo test -p policy-engine --lib action::dex
  cargo clippy -p policy-engine -- -D warnings

Commit message: "feat(policy-engine): add 7 DEX action variants (Phase 1.3)"
```

- [ ] **Verification:** All tests pass, clippy clean, workspace builds.

- [ ] **Review:** comprehensive-review:code-reviewer. Focus: field name/type vs schema JSON; required vs optional; Default impls for enrichment.

- [ ] **Commit:** Codex auto.

### Task 1.4: Lending action structs (9 variants)

**Files:**
- Create: `crates/policy-engine/src/action/lending.rs`
- Modify: `crates/policy-engine/src/action/mod.rs`

- [ ] **Codex Delegation**

```
TASK: Implement 9 Lending action variants per schema/schema/actions/lending/*.json.

Repo: /Users/woojin/Desktop/upside_academy/project/policy-engine
Branch: phase-1-action-types

Files to create: crates/policy-engine/src/action/lending.rs
Files to modify: crates/policy-engine/src/action/mod.rs (`pub mod lending;`)

Reference 9 schemas: supply, withdraw, borrow, repay, liquidate, flash_loan, set_authorization, sign_authorization, revoke.

For each action: struct with name `<Title>Action`, derives Debug/Clone/PartialEq/Eq/Serialize/Deserialize, serde rename_all = "camelCase".

Supporting types in this file:
  - MarketRef { address?: Address, id?: String, label?: String }
  - AmountMode enum: Assets, Shares
  - RepayKind enum: DebtAsset, AtokenDirect
  - LiquidateKind enum: PoolShare, ProtocolAbsorb, Socializable, SingleAsset
  - AuthorizationScope enum: All, DebtOnly, ManagerRole, PositionManagerRole
  - SignAuthorizationScope enum: All, DebtOnly, ManagerRole
  - RevokeKind enum: BurnAll, RevokeUserAllowance, RevokeDelegation, BurnAllowance

Action structs (field list mirrors JSON; consult schema files for exact required/optional):

1. SupplyAction: market, asset, amount, amount_mode, recipient, from?, validity?
2. WithdrawAction: market, asset, amount, amount_mode, recipient, on_behalf?
3. BorrowAction: market, asset, amount, amount_mode, recipient, on_behalf
4. RepayAction: market, asset, amount, amount_mode, repay_kind, from?, on_behalf?, validity?
5. LiquidateAction: market, collateral, debt, liquidator, target, amount_in, liquidate_kind, recipient?
6. FlashLoanAction: market, asset, amount, recipient, initiator
7. SetAuthorizationAction: market, delegatee, scope, validity
8. SignAuthorizationAction: market, delegatee, scope (SignAuthorizationScope), signature (Hex)
9. RevokeAction: market, revokee, revoke_kind, amount?

Tests: per-action minimal-and-full roundtrip, total 18 tests.

Run: cargo test -p policy-engine --lib action::lending

Commit: "feat(policy-engine): add 9 lending action variants (Phase 1.4)"
```

- [ ] **Verification:** tests pass, clippy clean.
- [ ] **Review:** code-reviewer. Focus: enum naming consistency, MarketRef vs PoolRef de-duplication potential (acceptable: keep separate, schema distinguishes them).
- [ ] **Commit:** Codex auto.

### Task 1.5: Misc action structs (10 variants)

**Files:**
- Create: `crates/policy-engine/src/action/misc.rs`
- Modify: `crates/policy-engine/src/action/mod.rs`

- [ ] **Codex Delegation**

```
TASK: Implement 10 Misc action variants per schema/schema/actions/misc/*.json.

Files to create: crates/policy-engine/src/action/misc.rs
Files to modify: crates/policy-engine/src/action/mod.rs (`pub mod misc;`)

Action structs (consult schema files for field details):

1. WrapAction: token_in: AssetRef, token_out: AssetRef, amount: AmountConstraint, recipient: Address
2. UnwrapAction: token_in: AssetRef, amount: AmountConstraint, recipient: Address, token_out: Option<AssetRef>
3. ApproveAction: token: AssetRef, spender: Address, spender_label: Option<String>, amount: AmountConstraint, approval_kind: ApprovalKind, current_allowance: Option<DecimalString>, validity: Option<Validity>
4. SetApprovalForAllAction: token: AssetRef, spender: Address, spender_label: Option<String>, approved: bool
5. TransferAction: token: AssetRef, from: Address, recipient: Address, amount: Option<AmountConstraint>, token_id: Option<DecimalString>
6. PermitAction: token: AssetRef, spender: Address, amount: AmountConstraint, permit_kind: PermitKind, deadline: DecimalString
7. ClaimRewardsAction: pool: PoolRef, reward_tokens: Vec<AssetRef>, amounts: Vec<AmountConstraint>, recipient: Address, claim_kind: Option<String>
8. SignMessageAction: message: Hex, message_kind: MessageKind
9. DelegateAction: token: AssetRef, delegatee: Address, amount: AmountConstraint
10. VoteAction: governance: Address, proposal_id: DecimalString, support: u8 (0/1/2), reason: Option<String>

Supporting enums (snake_case unless noted):
  - ApprovalKind: Erc20, Erc20Increase, Erc20Decrease, Permit2
  - PermitKind: Eip2612, Permit2Single, Permit2Transfer
  - MessageKind: RawMessage, TypedDataHash

Re-import PoolRef from `crate::action::dex` (or move PoolRef into common.rs if it appears in misc too — for now keep in dex and re-export).

Tests: 20 (10 minimal + 10 full).

Commit: "feat(policy-engine): add 10 misc action variants (Phase 1.5)"
```

- [ ] **Verification + Review + Commit:** standard cycle.

### Task 1.6: Staking + Restaking action structs (6 variants)

**Files:**
- Create: `crates/policy-engine/src/action/staking.rs`
- Create: `crates/policy-engine/src/action/restaking.rs`
- Modify: `crates/policy-engine/src/action/mod.rs`

- [ ] **Codex Delegation**

```
TASK: Implement 6 Staking + Restaking action variants per schema/schema/actions/{staking,restaking}/*.json.

Files:
  - Create crates/policy-engine/src/action/staking.rs
  - Create crates/policy-engine/src/action/restaking.rs
  - Modify mod.rs: `pub mod staking; pub mod restaking;`

Staking (3):
  1. StakeAction: token_in, receipt_token, amount_in, amount_out?, recipient
  2. RequestUnstakeAction: receipt_token, token_out, amount_in, amount_out?, ticket: TicketRef, recipient
  3. ClaimUnstakeAction: ticket: TicketRef, token_out, amount, recipient

Restaking (3):
  1. RestakeAction: token_in, receipt_token?, amount_in, amount_out?, strategy: StrategyRef, recipient
  2. RequestRestakeWithdrawalAction: receipt_token, amount_in, amount_out?, ticket: TicketRef, recipient
  3. ClaimRestakeWithdrawalAction: ticket: TicketRef, token_out?, amount, recipient

Supporting types:
  - TicketRef { nft?: AssetRef, token_id?: DecimalString, id?: String }
  - StrategyRef { address?: Address, id?: String, label?: String }

Tests: 12 (6 minimal + 6 full).

Commit: "feat(policy-engine): add 6 staking/restaking action variants (Phase 1.6)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 1.7: Action enum (32 variants) + ActionEnvelope wire up

**Files:**
- Modify: `crates/policy-engine/src/action/envelope.rs` (replace `Stub` placeholder with 32 real variants)
- Modify: `crates/policy-engine/src/action/mod.rs` (re-exports)

- [ ] **Codex Delegation**

```
TASK: Replace the placeholder Action::Stub with all 32 real variants.

File: crates/policy-engine/src/action/envelope.rs

Replace:
  pub enum Action { Stub }

With:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "fields", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]  // intentional; 32-variant flat enum
pub enum Action {
    // dex (7)
    Swap(crate::action::dex::SwapAction),
    AddLiquidity(crate::action::dex::AddLiquidityAction),
    RemoveLiquidity(crate::action::dex::RemoveLiquidityAction),
    MintLiquidityNft(crate::action::dex::MintLiquidityNftAction),
    BurnLiquidityNft(crate::action::dex::BurnLiquidityNftAction),
    IncreaseLiquidity(crate::action::dex::IncreaseLiquidityAction),
    DecreaseLiquidity(crate::action::dex::DecreaseLiquidityAction),
    // lending (9)
    Supply(crate::action::lending::SupplyAction),
    Withdraw(crate::action::lending::WithdrawAction),
    Borrow(crate::action::lending::BorrowAction),
    Repay(crate::action::lending::RepayAction),
    Liquidate(crate::action::lending::LiquidateAction),
    FlashLoan(crate::action::lending::FlashLoanAction),
    SetAuthorization(crate::action::lending::SetAuthorizationAction),
    SignAuthorization(crate::action::lending::SignAuthorizationAction),
    Revoke(crate::action::lending::RevokeAction),
    // misc (10)
    Wrap(crate::action::misc::WrapAction),
    Unwrap(crate::action::misc::UnwrapAction),
    Approve(crate::action::misc::ApproveAction),
    SetApprovalForAll(crate::action::misc::SetApprovalForAllAction),
    Transfer(crate::action::misc::TransferAction),
    Permit(crate::action::misc::PermitAction),
    ClaimRewards(crate::action::misc::ClaimRewardsAction),
    SignMessage(crate::action::misc::SignMessageAction),
    Delegate(crate::action::misc::DelegateAction),
    Vote(crate::action::misc::VoteAction),
    // staking (3)
    Stake(crate::action::staking::StakeAction),
    RequestUnstake(crate::action::staking::RequestUnstakeAction),
    ClaimUnstake(crate::action::staking::ClaimUnstakeAction),
    // restaking (3)
    Restake(crate::action::restaking::RestakeAction),
    RequestRestakeWithdrawal(crate::action::restaking::RequestRestakeWithdrawalAction),
    ClaimRestakeWithdrawal(crate::action::restaking::ClaimRestakeWithdrawalAction),
}
```

Add helper method:
```rust
impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Swap(_) => "swap",
            // ... all 32
        }
    }

    pub fn default_category(&self) -> Category {
        match self {
            Action::Swap(_) | Action::AddLiquidity(_) | Action::RemoveLiquidity(_)
                | Action::MintLiquidityNft(_) | Action::BurnLiquidityNft(_)
                | Action::IncreaseLiquidity(_) | Action::DecreaseLiquidity(_)
                => Category::Dex,
            Action::Supply(_) | Action::Withdraw(_) | Action::Borrow(_) | Action::Repay(_)
                | Action::Liquidate(_) | Action::FlashLoan(_)
                | Action::SetAuthorization(_) | Action::SignAuthorization(_) | Action::Revoke(_)
                => Category::Lending,
            Action::Stake(_) | Action::RequestUnstake(_) | Action::ClaimUnstake(_)
                => Category::LiquidStaking,
            Action::Restake(_) | Action::RequestRestakeWithdrawal(_) | Action::ClaimRestakeWithdrawal(_)
                => Category::Restaking,
            _ => Category::Misc,
        }
    }
}
```

Tests:
  - test_action_kind_returns_snake_case_name (all 32)
  - test_action_default_category_for_each_variant (all 32)
  - test_action_envelope_wire_format_for_swap (full roundtrip, must match schema/root.json shape)
  - test_action_envelope_wire_format_for_approve
  - test_action_envelope_wire_format_for_permit

Commit: "feat(policy-engine): wire all 32 Action variants into envelope (Phase 1.7)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 1.8: RootRequest top-level wrapper

**Files:**
- Create: `crates/policy-engine/src/root.rs`
- Modify: `crates/policy-engine/src/lib.rs` (re-export RootRequest)

- [ ] **Codex Delegation**

```
TASK: Add RootRequest mirroring schema/schema/root.json.

File to create: crates/policy-engine/src/root.rs
File to modify: crates/policy-engine/src/lib.rs

Required types:
  - RequestKind enum: Transaction, Signature, UserOperation (rename_all snake_case)
  - ProtocolRef { name: String, version?: String, component?: String }
  - RootRequest:
      schema_version: String (default "1.0.1"),
      request_kind: RequestKind,
      chain_id: u64,
      from: Address,
      to: Address,
      value: DecimalString,
      selector: String (8-hex prefixed "0x"),
      protocol: Option<ProtocolRef>,
      actions: Vec<ActionEnvelope>,
      block_timestamp: Option<u64>
  - Derive: Debug, Clone, PartialEq, Eq, Serialize, Deserialize. serde rename_all = "camelCase"
  - Const: pub const SCHEMA_VERSION: &str = "1.0.1";

Tests:
  - test_root_request_serde_minimal (empty actions, no protocol, no block_timestamp)
  - test_root_request_serde_full
  - test_root_request_round_trips_one_swap_action (use SwapAction from Task 1.3)

Commit: "feat(policy-engine): add RootRequest top-level wrapper (Phase 1.8)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 1.9: Rename existing Action → LegacyAction

**Files:**
- Modify: `crates/policy-engine/src/core.rs` (rename `Action` → `LegacyAction`)
- Modify: `crates/policy-engine/src/lib.rs` (export both `LegacyAction` and new `Action`)
- Modify: All call sites in `crates/policy-engine/src/{policy,pipeline,registry,adapter,lowering/*,host/*}.rs` (rename references)
- Modify: All call sites in `crates/policy_engine_wasm/`, `crates/web-server/`, `crates/integration-tests/`, `crates/adapters/*`, `crates/request-router/`, `crates/mappers/`, `crates/sign-resolver/`

- [ ] **Codex Delegation**

```
TASK: Rename `pub enum Action` (the old 5-variant one in core.rs) to `LegacyAction`. Keep all its variants and impls. This is purely a rename — semantics unchanged.

The NEW Action enum (32-variant in action/envelope.rs) coexists.

Approach:
  1. In crates/policy-engine/src/core.rs:
     - rename `pub enum Action` to `pub enum LegacyAction`
     - rename all `impl Action` blocks to `impl LegacyAction`
     - keep the discriminant strings ("dex", "other", "permit2", "eip2612", "eip712Other") in serde rename_all unchanged
  2. In crates/policy-engine/src/lib.rs:
     - export both: `pub use crate::core::LegacyAction; pub use crate::action::Action;`
  3. Global rename across workspace: every reference to the OLD `Action`/`crate::core::Action`/`policy_engine::Action` must become `LegacyAction`/`policy_engine::LegacyAction`. Use `cargo build` errors to drive a rename pass (sed + manual).
  4. Files affected (run a workspace grep for `Action` first to enumerate):
     - crates/policy-engine/src/{policy,pipeline,registry,adapter,context_keys,prelude,schema}.rs
     - crates/policy-engine/src/host/*.rs
     - crates/policy-engine/src/lowering/*.rs
     - crates/policy_engine_wasm/src/*.rs
     - crates/request-router/src/*.rs
     - crates/sign-resolver/src/*.rs
     - crates/abi-resolver/src/*.rs (if referenced)
     - crates/mappers/src/*.rs (if referenced)
     - crates/web-server/src/*.rs
     - crates/integration-tests/tests/*.rs
     - crates/adapters/{eip2612,permit2,uniswap-v2,uniswap-v3,universal-router}/src/*.rs

  IMPORTANT: the NEW 32-variant Action MUST NOT replace the LegacyAction call sites. Only the rename. After this task the codebase has TWO Action types coexisting:
    - policy_engine::LegacyAction (old 5-variant, still wired into pipeline)
    - policy_engine::Action (new 32-variant, not yet used)

Tests:
  - All existing tests must still pass (cargo test --workspace)
  - cargo clippy --workspace -- -D warnings
  - cargo build --workspace

Commit: "refactor(policy-engine): rename old 5-variant Action to LegacyAction (Phase 1.9)"

IMPORTANT: this is a HIGH-RISK task. Many files touched. Use codex's diff carefully. After the rename, run `git grep -wE '(::|^|\W)Action(\W|$)' -- '*.rs'` to spot residual references; every match should be either:
  - in crates/policy-engine/src/action/ (new Action — OK)
  - in policy_engine::action::Action context (new Action — OK)
  - imported from policy_engine::LegacyAction (legacy — OK)
  - need to be renamed (NOT OK)
```

- [ ] **Verification**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
git grep -wE '(::|^|\W)Action(\W|$)' -- '*.rs' | grep -v 'src/action/' | grep -v 'LegacyAction' | grep -v 'ActionEnvelope' | grep -v 'fn action' | grep -v '"action"'
# 마지막 grep 출력은 거의 비어야 함; 남는 매치는 사람이 검토하여 의도된 것인지 확인
```

- [ ] **Review:** code-reviewer. High-risk task — verify rename completeness, no semantic changes, both Action types coexist properly.

- [ ] **Commit:** Codex auto.

### Phase 1 Review Gate

- [ ] 모든 Task 1.1-1.9 통과
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace -- -D warnings` green
- [ ] `git grep -nE 'pub enum (Action|LegacyAction)\b' -- 'crates/**/*.rs'` 결과:
  - `crates/policy-engine/src/core.rs`: `pub enum LegacyAction`
  - `crates/policy-engine/src/action/envelope.rs`: `pub enum Action`
  - 그 외 0건
- [ ] Phase 0 baseline 과 회귀 비교: `./scripts/capture_baseline.sh` 다시 실행해서 `baseline_pre_refactor/` 와 diff — 모든 10 파일 동일해야 함 (LegacyAction 만 바뀌었고 wire format 무변화)
- [ ] PR open: `gh pr create --base feature/action-adapter-refactor --head phase-1-action-types --title "[Phase 1] Add 32-variant Action types (LegacyAction coexists)"`
- [ ] 사용자 명시 승인

---

# Phase 1.5 — Crate 위치 재배치

**목적:** abi-resolver, mappers, sign-resolver, request-router 4개 crate 를 `crates/adapters/` 하위로 이동. 코드/타입 변경 0, 경로 이동만.

**브랜치:** `phase-1_5-relocate-crates`

### Task 1.5.1: git mv 4개 + Cargo.toml 경로 갱신

**Files:**
- Move: `crates/abi-resolver/` → `crates/adapters/abi-resolver/`
- Move: `crates/mappers/` → `crates/adapters/mappers/`
- Move: `crates/sign-resolver/` → `crates/adapters/sign-resolver/`
- Move: `crates/request-router/` → `crates/adapters/request-router/`
- Modify: 루트 `Cargo.toml` (`[workspace] members` 4개 경로)
- Modify: 모든 의존 crate 의 `Cargo.toml` (path 경로)

- [ ] **Codex Delegation**

```
TASK: Move 4 crates under crates/adapters/ via git mv. Pure relocation — no code changes.

Repo: /Users/woojin/Desktop/upside_academy/project/policy-engine
Branch: phase-1_5-relocate-crates (from feature/action-adapter-refactor)

Commands to run (in order):
  git mv crates/abi-resolver crates/adapters/abi-resolver
  git mv crates/mappers crates/adapters/mappers
  git mv crates/sign-resolver crates/adapters/sign-resolver
  git mv crates/request-router crates/adapters/request-router

Update root Cargo.toml [workspace] members:
  - "crates/abi-resolver"           → "crates/adapters/abi-resolver"
  - "crates/mappers"                → "crates/adapters/mappers"
  - "crates/sign-resolver"          → "crates/adapters/sign-resolver"
  - "crates/request-router"         → "crates/adapters/request-router"
  (the existing "crates/adapters/*" entries for eip2612/permit2/uniswap-v2/uniswap-v3/universal-router stay)

Update path = "..." dependencies in the following Cargo.toml files:
  - crates/policy-engine/Cargo.toml
  - crates/policy_engine_wasm/Cargo.toml
  - crates/integration-tests/Cargo.toml
  - crates/web-server/Cargo.toml
  - crates/adapters/eip2612/Cargo.toml
  - crates/adapters/permit2/Cargo.toml
  - crates/adapters/uniswap-v2/Cargo.toml
  - crates/adapters/uniswap-v3/Cargo.toml
  - crates/adapters/universal-router/Cargo.toml
  - crates/adapters-bundle/Cargo.toml

For each, change relative paths:
  ../abi-resolver        → ../adapters/abi-resolver
  ../mappers             → ../adapters/mappers
  ../sign-resolver       → ../adapters/sign-resolver
  ../request-router      → ../adapters/request-router

NOTE: crate package.name in each moved Cargo.toml is UNCHANGED (still abi-resolver, mappers, sign-resolver, request-router). Only the path = "..." relative path strings change.

Verification:
  cargo build --workspace
  cargo test --workspace
  cargo clippy --workspace -- -D warnings

ALL must pass with zero code changes other than Cargo.toml path strings and the moved files.

Commit message: "refactor: nest abi-resolver/mappers/sign-resolver/request-router under crates/adapters/ (Phase 1.5)"

If any cargo build error is something OTHER than a path-string issue, STOP and report — it means a transitive consumer has a path string we missed.
```

- [ ] **Verification**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
ls crates/adapters/
# should show: abi-resolver  adapters-bundle  eip2612  mappers  permit2  request-router  sign-resolver  uniswap-v2  uniswap-v3  universal-router
ls crates/abi-resolver 2>&1
# should error: No such file or directory
```

- [ ] **Review:** code-reviewer. Focus: pure git mv (no code changes), Cargo.toml path string correctness.

- [ ] **Commit:** Codex auto. Then push.

### Phase 1.5 Review Gate

- [ ] 4개 crate `crates/adapters/` 아래에 존재
- [ ] 옛 위치 (`crates/abi-resolver` 등) 존재 X
- [ ] `cargo build --workspace`, `cargo test --workspace`, `cargo clippy -- -D warnings` 모두 green
- [ ] Phase 0 baseline 회귀 통과
- [ ] PR open, 사용자 승인

---

# Phase 2 — Decoder trait + abi-resolver 재정비

**목적:** `Decoder` trait + `DecoderRegistry` trait 도입. 기존 protocol-specific 디코딩 로직 (Uniswap V2/V3/V4, Universal Router) 을 trait 구현으로 wrap. 외부 API (`Resolver::resolve`) 호환 유지.

**브랜치:** `phase-2-decoder-trait`

### Task 2.1: Decoder trait + DecodedCall types

**Files:**
- Create: `crates/adapters/abi-resolver/src/decoder.rs`
- Modify: `crates/adapters/abi-resolver/src/lib.rs` (`pub mod decoder;`)

- [ ] **Codex Delegation**

```
TASK: Define Decoder trait + DecodedCall types in abi-resolver.

File: crates/adapters/abi-resolver/src/decoder.rs

Types to define:
  pub struct DecoderId(pub String);  // e.g. "uniswap-v2/swapExactTokensForTokens"
  - Derive Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize

  pub struct CallMatchKey {
      pub chain_id: u64,
      pub to: policy_engine::Address,
      pub selector: [u8; 4],
  }
  - Derive: Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize
  - Selector serializes as "0x" + 8 hex chars (custom serde)

  pub struct DecodedArg {
      pub name: String,
      pub abi_type: String,
      pub value: DecodedValue,
  }

  pub enum DecodedValue {
      Address(policy_engine::Address),
      Uint(alloy_primitives::U256),
      Int(alloy_primitives::I256),
      Bool(bool),
      Bytes(Vec<u8>),
      String(String),
      Array(Vec<DecodedValue>),
      Tuple(Vec<DecodedValue>),
  }

  pub struct DecodedCall {
      pub decoder_id: DecoderId,
      pub function_signature: String,  // e.g. "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)"
      pub args: Vec<DecodedArg>,
      pub nested: Vec<DecodedCall>,    // multicall / UR sub-calls
  }

  pub trait Decoder: Send + Sync {
      fn id(&self) -> DecoderId;
      fn match_keys(&self) -> Vec<CallMatchKey>;
      fn decode(&self, ctx: &DecodeContext<'_>, calldata: &[u8])
          -> Result<DecodedCall, DecoderError>;
  }

  pub struct DecodeContext<'a> {
      pub chain_id: u64,
      pub to: &'a policy_engine::Address,
      pub value: &'a policy_engine::DecimalString,
      pub block_timestamp: Option<u64>,
  }

  pub enum DecoderError {
      UnsupportedSelector,
      InvalidCalldata(String),
      AbiMismatch(String),
      Internal(anyhow::Error),
  }

  pub trait DecoderRegistry: Send + Sync {
      fn resolve(&self, key: &CallMatchKey) -> Option<std::sync::Arc<dyn Decoder>>;
      fn match_keys(&self) -> Vec<CallMatchKey>;  // for diagnostics
  }

Tests:
  - test_call_match_key_serde_with_hex_selector (("0x12345678") form)
  - test_decoded_call_nested_serde
  - test_decoder_error_display

Commit: "feat(abi-resolver): add Decoder + DecoderRegistry traits and types (Phase 2.1)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 2.2: InMemoryDecoderRegistry

**Files:**
- Create: `crates/adapters/abi-resolver/src/in_memory_registry.rs`
- Modify: `crates/adapters/abi-resolver/src/lib.rs`

- [ ] **Codex Delegation**

```
TASK: Implement InMemoryDecoderRegistry as the default DecoderRegistry impl.

File: crates/adapters/abi-resolver/src/in_memory_registry.rs

```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct InMemoryDecoderRegistry {
    by_key: HashMap<CallMatchKey, Arc<dyn Decoder>>,
}

impl InMemoryDecoderRegistry {
    pub fn builder() -> InMemoryDecoderRegistryBuilder { ... }
}

pub struct InMemoryDecoderRegistryBuilder {
    decoders: Vec<Arc<dyn Decoder>>,
}

impl InMemoryDecoderRegistryBuilder {
    pub fn register(mut self, decoder: Arc<dyn Decoder>) -> Self { ... }
    pub fn build(self) -> InMemoryDecoderRegistry {
        let mut by_key = HashMap::new();
        for d in self.decoders {
            for k in d.match_keys() {
                by_key.insert(k, d.clone());
            }
        }
        InMemoryDecoderRegistry { by_key }
    }
}

impl DecoderRegistry for InMemoryDecoderRegistry {
    fn resolve(&self, key: &CallMatchKey) -> Option<Arc<dyn Decoder>> {
        self.by_key.get(key).cloned()
    }
    fn match_keys(&self) -> Vec<CallMatchKey> {
        self.by_key.keys().cloned().collect()
    }
}
```

Tests:
  - test_in_memory_registry_register_and_resolve (mock Decoder, single match key)
  - test_in_memory_registry_multiple_decoders_distinct_keys
  - test_in_memory_registry_collision_last_wins (or panic? — choose panic for clarity, assert in test)

Commit: "feat(abi-resolver): add InMemoryDecoderRegistry (Phase 2.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 2.3: Migrate Uniswap V2/V3/V4 + Universal Router decoder modules

**Files:**
- Create: `crates/adapters/abi-resolver/src/decoders/uniswap_v2.rs`
- Create: `crates/adapters/abi-resolver/src/decoders/uniswap_v3.rs`
- Create: `crates/adapters/abi-resolver/src/decoders/uniswap_v4.rs`
- Create: `crates/adapters/abi-resolver/src/decoders/universal_router.rs`
- Modify: `crates/adapters/abi-resolver/src/lib.rs` (`pub mod decoders;`)

- [ ] **Codex Delegation**

```
TASK: Implement Decoder trait for 4 protocol families. Wrap existing decoding logic.

Files to create:
  - crates/adapters/abi-resolver/src/decoders/uniswap_v2.rs
  - crates/adapters/abi-resolver/src/decoders/uniswap_v3.rs
  - crates/adapters/abi-resolver/src/decoders/uniswap_v4.rs
  - crates/adapters/abi-resolver/src/decoders/universal_router.rs

Source material:
  - Existing decoding logic in crates/adapters/{uniswap-v2,uniswap-v3,universal-router}/src/*.rs
  - Existing subdecode logic in crates/adapters/abi-resolver/src/subdecode/protocols/
  - Use alloy_sol_types::sol! macro for ABI definitions

Each module:
  - Define one Decoder per function (or one Decoder per protocol with internal selector switching — choose per-function for testability)
  - Implement Decoder trait:
    - id() returns DecoderId("uniswap-v2/<function>") etc.
    - match_keys() returns ALL chain_id × router_address × selector combinations (multi-chain support: chain ids 1, 8453, 10, 42161, 137, plus per-chain router addresses)
    - decode() parses calldata using sol! and produces DecodedCall

Example for uniswap_v2::swap_exact_tokens_for_tokens:

```rust
use alloy_sol_types::sol;

sol! {
    function swapExactTokensForTokens(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[]);
}

pub struct SwapExactTokensForTokensDecoder;

impl Decoder for SwapExactTokensForTokensDecoder {
    fn id(&self) -> DecoderId {
        DecoderId("uniswap-v2/swapExactTokensForTokens".into())
    }
    fn match_keys(&self) -> Vec<CallMatchKey> {
        // ROUTERS_BY_CHAIN: const map of chain_id → router address
        ROUTERS_BY_CHAIN.iter().map(|(chain_id, router)| {
            CallMatchKey {
                chain_id: *chain_id,
                to: router.clone(),
                selector: swapExactTokensForTokensCall::SELECTOR,
            }
        }).collect()
    }
    fn decode(&self, ctx: &DecodeContext, calldata: &[u8]) -> Result<DecodedCall, DecoderError> {
        let call = swapExactTokensForTokensCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;
        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)".into(),
            args: vec![
                DecodedArg { name: "amountIn".into(), abi_type: "uint256".into(), value: DecodedValue::Uint(call.amountIn) },
                DecodedArg { name: "amountOutMin".into(), abi_type: "uint256".into(), value: DecodedValue::Uint(call.amountOutMin) },
                DecodedArg { name: "path".into(), abi_type: "address[]".into(), value: DecodedValue::Array(call.path.iter().map(|a| DecodedValue::Address(a.into())).collect()) },
                DecodedArg { name: "to".into(), abi_type: "address".into(), value: DecodedValue::Address(call.to.into()) },
                DecodedArg { name: "deadline".into(), abi_type: "uint256".into(), value: DecodedValue::Uint(call.deadline) },
            ],
            nested: vec![],
        })
    }
}
```

Repeat for:
  - V2: swapExactTokensForTokens, swapTokensForExactTokens, swapExactETHForTokens, swapExactTokensForETH, swapETHForExactTokens, swapTokensForExactETH (6)
  - V3: exactInputSingle, exactInput, exactOutputSingle, exactOutput, multicall (5)
  - V4: PoolManager.swap (entry point — may produce nested calls per command)
  - UniversalRouter: execute, executeWithDeadline (with command-stream sub-decoding into `nested: Vec<DecodedCall>`)

For Universal Router: the command stream parser already exists in crates/adapters/abi-resolver/src/subdecode/; refactor to be called from the UR Decoder.

Per-decoder tests: at least 2 golden-calldata fixtures from existing crates/adapters/<protocol>/tests/.

Commit: "feat(abi-resolver): add 4 protocol Decoder implementations (Phase 2.3)"
```

- [ ] **Verification**

```bash
cargo test -p abi-resolver --lib decoders -- --nocapture
cargo clippy -p abi-resolver -- -D warnings
```

- [ ] **Review:** code-reviewer. Focus: ROUTERS_BY_CHAIN completeness (chain coverage), DecodedArg.value type accuracy, V4/UR nested call handling.

- [ ] **Commit:** Codex auto.

### Task 2.4: Wire DecoderRegistry into existing Resolver

**Files:**
- Modify: `crates/adapters/abi-resolver/src/resolver.rs`

- [ ] **Codex Delegation**

```
TASK: Make Resolver internally consult DecoderRegistry before falling back to Sourcify/OpenChain.

File: crates/adapters/abi-resolver/src/resolver.rs

The existing `Resolver::resolve(chain_id, to, calldata)` returns `Resolved` from Sourcify/openchain. Add a DecoderRegistry as a first-tier fast path:

```rust
pub struct Resolver {
    decoders: Arc<dyn DecoderRegistry>,
    sourcify_in_memory: ...,
    sourcify_sqlite: ...,
    openchain: ...,
}

impl Resolver {
    pub fn resolve(&self, chain_id: u64, to: &Address, calldata: &[u8]) -> ResolveOutcome {
        if calldata.len() < 4 { return ResolveOutcome::NotFound; }
        let selector: [u8; 4] = calldata[0..4].try_into().unwrap();

        let key = CallMatchKey { chain_id, to: to.clone(), selector };
        if let Some(decoder) = self.decoders.resolve(&key) {
            let ctx = DecodeContext { chain_id, to, value: &"0".into(), block_timestamp: None };
            if let Ok(decoded) = decoder.decode(&ctx, calldata) {
                return ResolveOutcome::Resolved {
                    source: ResolveSource::WhitelistDecoder,
                    decoded,
                };
            }
        }

        // Existing Sourcify/openchain fallback unchanged
        ...
    }
}
```

Add `ResolveSource::WhitelistDecoder` variant. Ensure backward-compatible serde for existing API consumers.

Tests:
  - test_resolver_uses_decoder_registry_first_on_match (mock registry returns a decoder; sourcify never called)
  - test_resolver_falls_back_to_sourcify_when_no_decoder

Commit: "feat(abi-resolver): wire DecoderRegistry into Resolver fast path (Phase 2.4)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 2 Review Gate

- [ ] All Task 2.1-2.4 통과
- [ ] `cargo test --workspace`, clippy clean
- [ ] Phase 0 baseline 회귀 통과 (Resolver 결과 동일)
- [ ] grep: `git grep -nE 'trait DecoderRegistry' -- 'crates/**/*.rs'` → 1 매치 (abi-resolver/src/decoder.rs)
- [ ] PR + 사용자 승인

---

# Phase 3 — Mapper trait + mappers 재정비

**목적:** `Mapper` trait + `MapperRegistry` trait 도입. 기존 `mappers/types/` 삭제, `policy-engine::action` import. protocol-별 Mapper 구현 (현재 6 actions: swap/wrap/unwrap/approve/add_liquidity/remove_liquidity 만 fully wired).

**브랜치:** `phase-3-mapper-trait`

### Task 3.1: Mapper trait + MapperRegistry trait

**Files:**
- Create: `crates/adapters/mappers/src/mapper.rs`
- Create: `crates/adapters/mappers/src/in_memory_registry.rs`
- Modify: `crates/adapters/mappers/src/lib.rs`

- [ ] **Codex Delegation**

```
TASK: Define Mapper trait + InMemoryMapperRegistry.

Files:
  - Create crates/adapters/mappers/src/mapper.rs
  - Create crates/adapters/mappers/src/in_memory_registry.rs
  - Modify crates/adapters/mappers/src/lib.rs (mod declarations + re-exports)

Types:
  pub struct MapperId(pub String);  // e.g. "uniswap-v2/swap"

  pub struct MapperMatchKey {
      pub decoder_id: abi_resolver::DecoderId,
      // Optionally include protocol_id, function_name as redundant disambiguation
  }
  - Derive Debug/Clone/PartialEq/Eq/Hash/Serialize/Deserialize

  pub trait Mapper: Send + Sync {
      fn id(&self) -> MapperId;
      fn accepts(&self, decoded: &abi_resolver::DecodedCall) -> bool;
      fn map(&self, ctx: &MapContext<'_>, decoded: &abi_resolver::DecodedCall)
          -> Result<Vec<policy_engine::ActionEnvelope>, MapperError>;
  }

  pub struct MapContext<'a> {
      pub chain_id: u64,
      pub from: &'a policy_engine::Address,
      pub to: &'a policy_engine::Address,
      pub value_wei: &'a policy_engine::DecimalString,
      pub block_timestamp: Option<u64>,
      pub token_registry: &'a dyn TokenRegistry,
  }

  pub trait TokenRegistry: Send + Sync {
      fn lookup(&self, chain_id: u64, address: &policy_engine::Address)
          -> Option<TokenMetadata>;
  }

  pub struct TokenMetadata {
      pub symbol: String,
      pub decimals: u8,
  }

  pub enum MapperError {
      ArgumentMismatch(String),
      MissingArgument(String),
      Internal(anyhow::Error),
  }

  pub trait MapperRegistry: Send + Sync {
      fn resolve(&self, key: &MapperMatchKey) -> Option<Arc<dyn Mapper>>;
  }

  pub struct InMemoryMapperRegistry { ... }
  // builder pattern same as InMemoryDecoderRegistry from Task 2.2

Tests:
  - test_mapper_match_key_serde_roundtrip
  - test_in_memory_mapper_registry_register_and_resolve
  - mock implementations of Mapper + TokenRegistry for tests

Commit: "feat(mappers): add Mapper + MapperRegistry traits (Phase 3.1)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 3.2: Delete mappers::types, switch to policy_engine::action

**Files:**
- Delete: `crates/adapters/mappers/src/types/` (entire directory)
- Modify: `crates/adapters/mappers/src/lib.rs` (remove `pub mod types;`)
- Modify: All files in `crates/adapters/mappers/src/` referencing `crate::types::*` → use `policy_engine::action::*` and `policy_engine::ActionEnvelope`

- [ ] **Codex Delegation**

```
TASK: Delete mappers::types module entirely, replacing internal uses with policy_engine::action types.

Repo: /Users/woojin/Desktop/upside_academy/project/policy-engine
Branch: phase-3-mapper-trait

Steps:
  1. Add policy-engine as a dependency to crates/adapters/mappers/Cargo.toml (likely already present)
  2. Delete crates/adapters/mappers/src/types/ recursively (git rm -r)
  3. In crates/adapters/mappers/src/lib.rs: remove `pub mod types;` and any `pub use types::*;`
  4. For each .rs file in crates/adapters/mappers/src/ that referenced types:
     - replace `crate::types::SwapAction` → `policy_engine::action::dex::SwapAction`
     - replace `crate::types::WrapAction` → `policy_engine::action::misc::WrapAction`
     - replace `crate::types::UnwrapAction` → `policy_engine::action::misc::UnwrapAction`
     - replace `crate::types::ApproveAction` → `policy_engine::action::misc::ApproveAction`
     - replace `crate::types::AddLiquidityAction` → `policy_engine::action::dex::AddLiquidityAction`
     - replace `crate::types::RemoveLiquidityAction` → `policy_engine::action::dex::RemoveLiquidityAction`
     - replace `crate::types::ActionFields` enum → `policy_engine::Action`
     - replace `crate::types::ActionEnvelope` → `policy_engine::ActionEnvelope`
     - replace `crate::types::RootRequest` → `policy_engine::RootRequest`
     - replace `crate::types::Category` → `policy_engine::Category`
     - replace `crate::types::AssetRef` → `policy_engine::action::common::AssetRef`
     - replace `crate::types::AmountConstraint` → `policy_engine::action::common::AmountConstraint`
     - replace `crate::types::Address` → `policy_engine::action::common::Address`
  5. Field/type compatibility check: the new SwapAction in policy_engine has extra `enrichment` field (vs old types::SwapAction). Mappers should populate it as `SwapEnrichment::default()` (empty) for now — host enrichment happens elsewhere.
  6. The new SwapAction has `mode: SwapMode` with 4 variants including `Market`; old types had only 3. Where mappers emitted `SwapMode::Unknown`, leave as-is.
  7. The new SwapAction has `validity: Option<Validity>` (struct) instead of old `deadline_seconds_from_now: Option<i64>`. Convert: `if let Some(secs) = old_value { Some(Validity { expires_at: format!("{}", block_timestamp + secs), source: ValiditySource::TxDeadline }) } else { None }`.

Verification:
  cargo build -p mappers
  cargo build --workspace
  cargo test --workspace

Commit: "refactor(mappers): replace mappers::types with policy_engine::action (Phase 3.2)"
```

- [ ] **Verification + Review + Commit:** standard. Review focus: deadline → Validity conversion correctness.

### Task 3.3: Migrate protocol mappers to Mapper trait

**Files:**
- Modify: `crates/adapters/mappers/src/protocols/uniswap_v2/*.rs`
- Modify: `crates/adapters/mappers/src/protocols/uniswap_v3/*.rs`
- Modify: `crates/adapters/mappers/src/protocols/uniswap_v4/*.rs`
- Modify: `crates/adapters/mappers/src/protocols/universal_router/*.rs`
- Modify: `crates/adapters/mappers/src/registry.rs` → replace with `InMemoryMapperRegistry`

- [ ] **Codex Delegation**

```
TASK: Refactor existing mapper modules to implement the Mapper trait.

For each protocol mapper (uniswap_v2, uniswap_v3, uniswap_v4, universal_router):
  - Replace existing entry function (e.g. `fn map_swap(ctx, decoded) -> Vec<ActionEnvelope>`) with a struct implementing Mapper:

    pub struct UniswapV2SwapMapper;
    impl Mapper for UniswapV2SwapMapper {
        fn id(&self) -> MapperId { MapperId("uniswap-v2/swap".into()) }
        fn accepts(&self, decoded: &DecodedCall) -> bool {
            decoded.decoder_id.0.starts_with("uniswap-v2/swap")
        }
        fn map(&self, ctx: &MapContext, decoded: &DecodedCall)
            -> Result<Vec<ActionEnvelope>, MapperError> {
            // Existing logic, but read from decoded.args[N] instead of raw calldata
            ...
        }
    }

Coverage scope for this task: produce Action variants for the 6 already-mapped actions:
  - Action::Swap
  - Action::Wrap
  - Action::Unwrap
  - Action::Approve
  - Action::AddLiquidity
  - Action::RemoveLiquidity

The remaining 26 Action variants land in follow-up PRs (out of scope for Phase 3).

Update crates/adapters/mappers/src/registry.rs to use InMemoryMapperRegistry. Old dispatch fn replaced with registry lookup.

Tests:
  - For each mapper: at least 1 happy-path golden test using a DecodedCall fixture
  - test_mapper_registry_dispatch_returns_correct_mapper

Commit: "feat(mappers): migrate protocol mappers to Mapper trait (Phase 3.3)"
```

- [ ] **Verification + Review + Commit:** standard. Review focus: ensure no Action variants beyond the 6 are emitted (other 26 are type-defined but not yet wired in mappers).

### Phase 3 Review Gate

- [ ] All Task 3.1-3.3 통과
- [ ] `crates/adapters/mappers/src/types/` 디렉토리 존재 X
- [ ] `git grep -n 'crate::types::' -- 'crates/adapters/mappers/**/*.rs'` → 0 매치
- [ ] `cargo test --workspace` green
- [ ] Phase 0 baseline 회귀: web-server `/api/decode` 응답이 (Action variant 이름 변경 외에) 의미적으로 동일 — Phase 7 까지는 wire format 변경 없으므로 모든 baseline 통과
- [ ] PR + 사용자 승인

---

# Phase 3.5 — CallAdapter composite + call-adapter crate

**목적:** 신규 `crates/adapters/call-adapter/` crate. `CallAdapter` trait + `DefaultCallAdapter` (Decoder→Mapper 자동 조립) + `InMemoryCallAdapterRegistry`.

**브랜치:** `phase-3_5-call-adapter`

### Task 3.5.1: 신규 crate 스캐폴드

**Files:**
- Create: `crates/adapters/call-adapter/Cargo.toml`
- Create: `crates/adapters/call-adapter/src/lib.rs`
- Modify: 루트 `Cargo.toml` ([workspace] members 추가)

- [ ] **Codex Delegation**

```
TASK: Scaffold new call-adapter crate.

Files:
  - Create crates/adapters/call-adapter/Cargo.toml:

```toml
[package]
name = "call-adapter"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
abi-resolver = { path = "../abi-resolver" }
mappers = { path = "../mappers" }
policy-engine = { path = "../../policy-engine" }
serde = { workspace = true, features = ["derive"] }
thiserror = { workspace = true }
anyhow = { workspace = true }
```

  - Create crates/adapters/call-adapter/src/lib.rs:
```rust
//! CallAdapter — composite over Decoder + Mapper.
//! Output: Vec<ActionEnvelope>. Symmetric to SignAdapter.

pub mod call_adapter;
pub mod default;
pub mod in_memory;

pub use call_adapter::{CallAdapter, CallAdapterId, CallAdapterRegistry, CallContext, AdapterError};
pub use default::DefaultCallAdapter;
pub use in_memory::InMemoryCallAdapterRegistry;
```

  - Add to root Cargo.toml [workspace.members]:
    "crates/adapters/call-adapter",

Verification:
  cargo build -p call-adapter
  cargo build --workspace

Commit: "feat(call-adapter): scaffold new crate (Phase 3.5.1)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 3.5.2: CallAdapter trait + DefaultCallAdapter

**Files:**
- Create: `crates/adapters/call-adapter/src/call_adapter.rs`
- Create: `crates/adapters/call-adapter/src/default.rs`

- [ ] **Codex Delegation**

```
TASK: Implement CallAdapter trait and DefaultCallAdapter.

File: crates/adapters/call-adapter/src/call_adapter.rs

```rust
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CallAdapterId(pub String);

pub struct CallContext<'a> {
    pub chain_id: u64,
    pub from: &'a policy_engine::Address,
    pub to: &'a policy_engine::Address,
    pub value_wei: &'a policy_engine::DecimalString,
    pub block_timestamp: Option<u64>,
    pub token_registry: &'a dyn mappers::TokenRegistry,
    pub decoder_registry: &'a dyn abi_resolver::DecoderRegistry,
    pub mapper_registry: &'a dyn mappers::MapperRegistry,
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("no decoder found for selector")]
    NoDecoder,
    #[error("no mapper found for decoded call")]
    NoMapper,
    #[error("decoder error: {0}")]
    Decoder(#[from] abi_resolver::DecoderError),
    #[error("mapper error: {0}")]
    Mapper(#[from] mappers::MapperError),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub trait CallAdapter: Send + Sync {
    fn id(&self) -> CallAdapterId;
    fn match_keys(&self) -> Vec<abi_resolver::CallMatchKey>;
    fn build(&self, ctx: &CallContext<'_>, calldata: &[u8])
        -> Result<Vec<policy_engine::ActionEnvelope>, AdapterError>;
}

pub trait CallAdapterRegistry: Send + Sync {
    fn resolve(&self, key: &abi_resolver::CallMatchKey) -> Option<Arc<dyn CallAdapter>>;
}
```

File: crates/adapters/call-adapter/src/default.rs

```rust
use std::sync::Arc;

/// Default composite: looks up Decoder, decodes, looks up Mapper, maps.
/// 99% of protocols use this; Universal Router etc. implement CallAdapter directly.
pub struct DefaultCallAdapter {
    id: CallAdapterId,
    match_keys: Vec<abi_resolver::CallMatchKey>,
}

impl DefaultCallAdapter {
    pub fn new(id: CallAdapterId, match_keys: Vec<abi_resolver::CallMatchKey>) -> Self {
        Self { id, match_keys }
    }
}

impl CallAdapter for DefaultCallAdapter {
    fn id(&self) -> CallAdapterId { self.id.clone() }
    fn match_keys(&self) -> Vec<abi_resolver::CallMatchKey> { self.match_keys.clone() }

    fn build(&self, ctx: &CallContext, calldata: &[u8])
        -> Result<Vec<policy_engine::ActionEnvelope>, AdapterError>
    {
        if calldata.len() < 4 { return Err(AdapterError::Invalid("calldata < 4 bytes".into())); }
        let selector: [u8; 4] = calldata[0..4].try_into().unwrap();
        let key = abi_resolver::CallMatchKey { chain_id: ctx.chain_id, to: ctx.to.clone(), selector };

        let decoder = ctx.decoder_registry.resolve(&key).ok_or(AdapterError::NoDecoder)?;
        let dec_ctx = abi_resolver::DecodeContext {
            chain_id: ctx.chain_id, to: ctx.to,
            value: ctx.value_wei, block_timestamp: ctx.block_timestamp,
        };
        let decoded = decoder.decode(&dec_ctx, calldata)?;

        let mapper_key = mappers::MapperMatchKey { decoder_id: decoded.decoder_id.clone() };
        let mapper = ctx.mapper_registry.resolve(&mapper_key).ok_or(AdapterError::NoMapper)?;
        let map_ctx = mappers::MapContext {
            chain_id: ctx.chain_id, from: ctx.from, to: ctx.to,
            value_wei: ctx.value_wei, block_timestamp: ctx.block_timestamp,
            token_registry: ctx.token_registry,
        };
        let envelopes = mapper.map(&map_ctx, &decoded)?;
        Ok(envelopes)
    }
}
```

Tests:
  - test_default_call_adapter_happy_path (mock decoder + mock mapper produce ActionEnvelope)
  - test_default_call_adapter_no_decoder_returns_error
  - test_default_call_adapter_no_mapper_returns_error
  - test_default_call_adapter_calldata_too_short

Commit: "feat(call-adapter): add CallAdapter trait + DefaultCallAdapter (Phase 3.5.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 3.5.3: InMemoryCallAdapterRegistry + factory for default registry

**Files:**
- Create: `crates/adapters/call-adapter/src/in_memory.rs`

- [ ] **Codex Delegation**

```
TASK: InMemoryCallAdapterRegistry and a factory that builds the default registry containing all DefaultCallAdapter entries derived from a DecoderRegistry.

File: crates/adapters/call-adapter/src/in_memory.rs

```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct InMemoryCallAdapterRegistry {
    by_key: HashMap<abi_resolver::CallMatchKey, Arc<dyn CallAdapter>>,
}

pub struct InMemoryCallAdapterRegistryBuilder {
    adapters: Vec<Arc<dyn CallAdapter>>,
}

impl InMemoryCallAdapterRegistry {
    pub fn builder() -> InMemoryCallAdapterRegistryBuilder { ... }

    /// Convenience: build a default registry that maps every (chain_id, to, selector)
    /// in the DecoderRegistry to a DefaultCallAdapter. Useful for the common case.
    pub fn from_decoder_registry(decoder_reg: &dyn abi_resolver::DecoderRegistry)
        -> Self
    {
        let mut builder = Self::builder();
        for key in decoder_reg.match_keys() {
            let adapter = DefaultCallAdapter::new(
                CallAdapterId(format!("default/{:?}", key)),
                vec![key],
            );
            builder = builder.register(Arc::new(adapter));
        }
        builder.build()
    }
}

impl CallAdapterRegistry for InMemoryCallAdapterRegistry {
    fn resolve(&self, key: &abi_resolver::CallMatchKey) -> Option<Arc<dyn CallAdapter>> {
        self.by_key.get(key).cloned()
    }
}
```

Tests:
  - test_in_memory_call_adapter_registry_resolves
  - test_from_decoder_registry_creates_default_adapter_per_match_key

Commit: "feat(call-adapter): add InMemoryCallAdapterRegistry (Phase 3.5.3)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 3.5 Review Gate

- [ ] All Task 3.5.1-3.5.3 통과
- [ ] `cargo test -p call-adapter` green
- [ ] `cargo test --workspace` green
- [ ] No regression in baseline
- [ ] PR + 사용자 승인

---

# Phase 4 — SignAdapter trait + sign-resolver 재정비

**목적:** `SignAdapter` trait + `SignAdapterRegistry`. eip2612 / permit2 adapter 로직을 `sign-resolver/src/adapters/` 로 이전.

**브랜치:** `phase-4-sign-adapter`

### Task 4.1: SignAdapter trait + SignAdapterRegistry

**Files:**
- Create: `crates/adapters/sign-resolver/src/sign_adapter.rs`
- Create: `crates/adapters/sign-resolver/src/in_memory.rs`
- Modify: `crates/adapters/sign-resolver/src/lib.rs`

- [ ] **Codex Delegation**

```
TASK: Define SignAdapter trait + InMemorySignAdapterRegistry.

Files:
  - Create crates/adapters/sign-resolver/src/sign_adapter.rs
  - Create crates/adapters/sign-resolver/src/in_memory.rs
  - Modify crates/adapters/sign-resolver/src/lib.rs (declare modules, re-export)

Types:
  pub struct SignAdapterId(pub String);

  #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub struct SignMatchKey {
      pub chain_id: u64,
      pub verifying_contract: Option<policy_engine::Address>,  // None = any (e.g. EIP-2612 wildcards)
      pub primary_type: String,                                 // EIP-712 primaryType
  }

  pub struct SignContext<'a> {
      pub chain_id: u64,
      pub signer: &'a policy_engine::Address,
      pub block_timestamp: Option<u64>,
      pub token_registry: &'a dyn mappers::TokenRegistry,
  }

  pub trait SignAdapter: Send + Sync {
      fn id(&self) -> SignAdapterId;
      fn match_keys(&self) -> Vec<SignMatchKey>;
      fn build(&self, ctx: &SignContext<'_>, sig: &crate::SignRequest)
          -> Result<Vec<policy_engine::ActionEnvelope>, crate::SignAdapterError>;
  }

  pub trait SignAdapterRegistry: Send + Sync {
      fn resolve(&self, key: &SignMatchKey) -> Option<Arc<dyn SignAdapter>>;
  }

  pub enum SignAdapterError {
      UnsupportedSchema,
      InvalidTypedData(String),
      MissingField(String),
      Internal(anyhow::Error),
  }

  pub struct InMemorySignAdapterRegistry { ... }
  // builder pattern as before

Tests:
  - test_sign_match_key_serde_roundtrip
  - test_in_memory_sign_adapter_registry

Commit: "feat(sign-resolver): add SignAdapter + SignAdapterRegistry traits (Phase 4.1)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 4.2: EIP-2612 SignAdapter

**Files:**
- Create: `crates/adapters/sign-resolver/src/adapters/eip2612.rs`
- Modify: `crates/adapters/sign-resolver/src/adapters/mod.rs`

- [ ] **Codex Delegation**

```
TASK: Implement EIP-2612 SignAdapter. Source from crates/adapters/eip2612/.

File: crates/adapters/sign-resolver/src/adapters/eip2612.rs

Behavior:
  - Match key: any chain_id, any verifying_contract, primary_type = "Permit"
  - On build():
    1. Parse SignRequest.payload as SignPayload::TypedData
    2. Validate primaryType == "Permit"
    3. Validate domain.name + domain.version + Permit type fields
    4. Extract: owner, spender, value, nonce, deadline
    5. Emit policy_engine::Action::Permit (misc::PermitAction):
         token: AssetRef { kind: Erc20, chain_id, address: domain.verifyingContract, ... }
         spender: spender,
         amount: AmountConstraint { kind: if value == MAX_U256 { Unlimited } else { Exact }, value: Some(...) }
         permit_kind: PermitKind::Eip2612
         deadline: deadline.to_string()
    6. Wrap in ActionEnvelope { category: Category::Misc, action: ... }

Source code to migrate: crates/adapters/eip2612/src/lib.rs

Tests:
  - test_eip2612_match_keys (returns single key with wildcard chain_id?)
  - test_eip2612_build_happy_path (use existing fixture from crates/adapters/eip2612/tests/)
  - test_eip2612_unlimited_amount_kind (value == MAX_U256 → kind: Unlimited)
  - test_eip2612_rejects_wrong_primary_type

Commit: "feat(sign-resolver): add EIP-2612 SignAdapter (Phase 4.2)"
```

- [ ] **Verification + Review + Commit:** standard. Review focus: faithful migration; output Action::Permit vs old Action::Eip2612 — should produce semantically equivalent data.

### Task 4.3: Permit2 SignAdapter

**Files:**
- Create: `crates/adapters/sign-resolver/src/adapters/permit2.rs`

- [ ] **Codex Delegation**

```
TASK: Implement Permit2 SignAdapter for 6 EIP-712 schemas (PermitSingle, PermitBatch, PermitTransferFrom, PermitBatchTransferFrom, PermitWitnessTransferFrom, PermitBatchWitnessTransferFrom).

Source: crates/adapters/permit2/src/.

File: crates/adapters/sign-resolver/src/adapters/permit2.rs

Match keys: 6 entries, each for a (chain_id wildcard, verifying_contract = Permit2 universal address 0x000000000022D473030F116dDEE9F6B43aC78BA3, primary_type = ...).

build() for each schema:
  - PermitSingle: 1 ActionEnvelope = Action::Permit { permit_kind: Permit2Single, ... }
  - PermitBatch: N ActionEnvelopes (one per permit in batch)
  - PermitTransferFrom: 1 ActionEnvelope = Action::Permit { permit_kind: Permit2Transfer, ... }
  - PermitBatchTransferFrom: N envelopes
  - PermitWitnessTransferFrom: 1 envelope (witness data attached as enrichment or ignored)
  - PermitBatchWitnessTransferFrom: N envelopes

Tests:
  - test_permit2_permit_single_build
  - test_permit2_permit_batch_emits_n_envelopes
  - test_permit2_permit_transfer_from_build
  - All 6 schemas tested with fixtures from crates/adapters/permit2/tests/

Commit: "feat(sign-resolver): add Permit2 SignAdapter (Phase 4.3)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 4 Review Gate

- [ ] All Task 4.1-4.3 통과
- [ ] `cargo test -p sign-resolver` green, including sample EIP-2612/Permit2 fixtures
- [ ] baseline 회귀: sign-relevant baseline 출력이 — Action variant 이름 변경 외 — 의미 동일
- [ ] PR + 사용자 승인

---

# Phase 5 — 옛 adapter sub-crate 삭제 + request-router 대칭 dispatch

**목적:** `crates/adapters/{eip2612,permit2,uniswap-v2,uniswap-v3,universal-router}` 5개 sub-crate + `crates/adapters-bundle` 삭제. request-router 가 `CallAdapter` + `SignAdapter` 두 composite trait 만 사용하도록 변경.

**브랜치:** `phase-5-delete-old-adapters`

### Task 5.1: request-router 가 CallAdapter + SignAdapter 사용하도록 변경

**Files:**
- Modify: `crates/adapters/request-router/src/lib.rs`
- Modify: `crates/adapters/request-router/src/transaction.rs`
- Modify: `crates/adapters/request-router/src/signature.rs`
- Modify: `crates/adapters/request-router/Cargo.toml` (의존성: call-adapter, sign-resolver)

- [ ] **Codex Delegation**

```
TASK: Rewire request-router to consume CallAdapterRegistry + SignAdapterRegistry directly.

File: crates/adapters/request-router/Cargo.toml
  Dependencies: call-adapter, sign-resolver, policy-engine, mappers (for TokenRegistry trait). Remove abi-resolver if no longer directly used.

File: crates/adapters/request-router/src/lib.rs
  New top-level entry point:

```rust
pub struct RouterContext<'a> {
    pub call_adapters: &'a dyn call_adapter::CallAdapterRegistry,
    pub sign_adapters: &'a dyn sign_resolver::SignAdapterRegistry,
    pub decoder_registry: &'a dyn abi_resolver::DecoderRegistry,
    pub mapper_registry: &'a dyn mappers::MapperRegistry,
    pub token_registry: &'a dyn mappers::TokenRegistry,
    pub block_timestamp: Option<u64>,
}

pub fn route_request(
    ctx: &RouterContext,
    method: &str,
    params: serde_json::Value,
    chain_id: u64,
) -> Result<policy_engine::RootRequest, RouterError> {
    match classify(method) {
        MethodClass::Call => transaction::route(ctx, method, params, chain_id),
        MethodClass::Sign => signature::route(ctx, method, params, chain_id),
        MethodClass::Unsupported => Err(RouterError::Unsupported(method.into())),
    }
}
```

File: crates/adapters/request-router/src/transaction.rs
  - Parse params as TransactionRequest (existing logic)
  - Build CallMatchKey from (chain_id, to, selector)
  - Resolve CallAdapter via ctx.call_adapters
  - Call adapter.build(&CallContext { ... }, &calldata)
  - Wrap result in RootRequest

File: crates/adapters/request-router/src/signature.rs
  - Parse params as SignRequest (existing logic — sign-resolver/src/payload.rs::parse_sign_request)
  - For TypedData: extract primary_type → SignMatchKey
  - Resolve SignAdapter via ctx.sign_adapters
  - Call adapter.build(&SignContext { ... }, &sig_request)
  - Wrap in RootRequest

CRITICAL CONSTRAINT: request-router code MUST NOT directly call abi-resolver::Decoder or mappers::Mapper. Only CallAdapter and SignAdapter. (Verification: grep, see Phase 5 review gate.)

Tests:
  - test_route_request_dispatches_call_for_eth_sendTransaction
  - test_route_request_dispatches_sign_for_eth_signTypedData_v4
  - test_route_request_unsupported_method

Commit: "refactor(request-router): use CallAdapter + SignAdapter symmetric dispatch (Phase 5.1)"
```

- [ ] **Verification + Review + Commit:** standard. Review focus: encapsulation — no Decoder/Mapper imports in request-router.

### Task 5.2: Build default registries factory in policy_engine_wasm + integration-tests

**Files:**
- Modify: `crates/policy_engine_wasm/src/lib.rs`
- Modify: `crates/integration-tests/src/lib.rs` (or new fixture module)

- [ ] **Codex Delegation**

```
TASK: Provide a `build_default_registries()` factory that constructs:
  - InMemoryDecoderRegistry with all 4 protocol decoders registered
  - InMemoryMapperRegistry with all 6 protocol mappers
  - InMemoryCallAdapterRegistry::from_decoder_registry(...)
  - InMemorySignAdapterRegistry with eip2612 + permit2 SignAdapters

Location: a shared "adapters-bundle"-style replacement. Two options:
  A) Add a small `crates/adapters/registries/` crate
  B) Inline in policy_engine_wasm + integration-tests separately

Choose option B (avoid new crate; the function lives in policy_engine_wasm and is duplicated in integration-tests). Helper signature:

```rust
pub fn build_default_registries() -> (
    InMemoryDecoderRegistry,
    InMemoryMapperRegistry,
    InMemoryCallAdapterRegistry,
    InMemorySignAdapterRegistry,
)
```

Tests:
  - test_default_registries_resolve_uniswap_v2_swap (build a known calldata, route_request, expect Action::Swap envelope)
  - test_default_registries_resolve_eip2612 (build a known typed data, expect Action::Permit envelope)

Commit: "feat: build_default_registries factory (Phase 5.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 5.3: Delete old adapter sub-crates

**Files:**
- Delete: `crates/adapters/eip2612/` (entire directory)
- Delete: `crates/adapters/permit2/` (entire directory)
- Delete: `crates/adapters/uniswap-v2/` (entire directory)
- Delete: `crates/adapters/uniswap-v3/` (entire directory)
- Delete: `crates/adapters/universal-router/` (entire directory)
- Delete: `crates/adapters-bundle/` (entire directory)
- Modify: 루트 `Cargo.toml` (remove 6 workspace members)
- Modify: All consumers' `Cargo.toml` (remove dependencies)

- [ ] **Codex Delegation**

```
TASK: Delete the 6 obsolete adapter sub-crates.

Steps:
  1. git rm -r crates/adapters/eip2612
  2. git rm -r crates/adapters/permit2
  3. git rm -r crates/adapters/uniswap-v2
  4. git rm -r crates/adapters/uniswap-v3
  5. git rm -r crates/adapters/universal-router
  6. git rm -r crates/adapters-bundle

  7. Update root Cargo.toml: remove 6 entries from [workspace] members

  8. For each Cargo.toml that depended on these (grep first):
     grep -lr 'eip2612\|permit2\|uniswap-v2\|uniswap-v3\|universal-router\|adapters-bundle' --include=Cargo.toml
     Remove the [dependencies] entries.

  9. For each .rs that imported these (grep next):
     grep -rn 'use \(policy_engine_adapter_\(eip2612\|permit2\|uniswap_v2\|uniswap_v3\|universal_router\)\|adapters_bundle\)' crates/
     Remove the use statements + any code that referenced removed types.

  10. In particular: crates/policy-engine/src/adapter.rs — delete the TransactionActionAdapter and SignatureActionAdapter trait definitions (they're now obsolete; CallAdapter + SignAdapter replace them).
      Also delete trait-related types: ActionAdapterId, TransactionActionAdapterDescriptor, SignatureActionAdapterDescriptor, DeclaredSignatureActionAdapter, DeclaredTransactionActionAdapter.
      The pipeline.rs and registry.rs in policy-engine still reference these — they'll be cleaned up in Phase 6 when PolicyRequest is trimmed to Swap-only. For now, may need to gate behind #[cfg(feature = "legacy")] or just delete the broken code and accept temporary compile errors in pipeline.rs/registry.rs that Phase 6 fixes.

  ACTUALLY: to keep CI green between Phase 5 and Phase 6, prefer to LEAVE policy-engine::adapter trait definitions stubbed out (compile-shimmed with #[deprecated] marker) until Phase 6 cleanup. Specifically:
    - Keep struct definitions for now
    - Remove all impl blocks in deleted crates (they're deleted anyway)
    - Keep pipeline.rs/registry.rs working with stubs that always return errors

Verification:
  cargo build --workspace
  cargo test --workspace
  cargo clippy --workspace -- -D warnings
  ls crates/adapters/
    Expected: abi-resolver  call-adapter  mappers  request-router  sign-resolver  (5 dirs)
  ls crates/adapters-bundle
    Expected: ENOENT

Commit: "refactor: delete obsolete adapter sub-crates (Phase 5.3)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 5 Review Gate

- [ ] `ls crates/adapters/` → 정확히 `abi-resolver  call-adapter  mappers  request-router  sign-resolver` (5 dirs)
- [ ] `ls crates/adapters-bundle` → 존재 X
- [ ] `cargo test --workspace` green
- [ ] **encapsulation check**: `git grep -nE 'use (abi_resolver::Decoder|mappers::Mapper)' -- 'crates/adapters/request-router/**/*.rs'` → 0 매치
- [ ] `git grep -nE 'TransactionActionAdapter|SignatureActionAdapter' -- 'crates/policy-engine/**/*.rs'` 결과: deprecated 스텁 외 사용 0
- [ ] baseline 회귀 통과
- [ ] PR + 사용자 승인

---

# Phase 6 — Swap-only PolicyRequest + LegacyAction 제거

**목적:** `PolicyRequest` lowering 을 `Action::Swap` 만 처리하도록 단순화. signature policies + 잉여 cedarschemas 삭제. `LegacyAction` 제거.

**브랜치:** `phase-6-swap-only-policy`

### Task 6.1: Swap-only lowering

**Files:**
- Modify: `crates/policy-engine/src/policy.rs` (request_from_action 단순화)
- Modify: `crates/policy-engine/src/lowering/` (Dex 외 모듈 제거)
- Modify: `crates/policy-engine/src/pipeline.rs` (Action::Swap 외 → Verdict::Unsupported)

- [ ] **Codex Delegation**

```
TASK: Trim PolicyRequest lowering to Action::Swap only. Other Action variants flow through pipeline without Cedar evaluation.

Files:
  - crates/policy-engine/src/policy.rs
    - Simplify request_from_action(action: &Action) -> Option<PolicyRequest>
    - Only Action::Swap returns Some; all others return None
    - For Action::Swap: lower to DexContext-shaped PolicyRequest. NOTE: existing DexContext is multi-hop (Set<Token>); the new single-hop SwapAction has token_in/token_out singletons → wrap them in 1-element Sets for Cedar compatibility. See compatibility note below.

  - Delete crates/policy-engine/src/lowering/ subdirs for non-dex actions (permit2.rs, eip2612.rs, eip712_other.rs, other.rs)
  - Keep crates/policy-engine/src/lowering/dex.rs but rewrite request_from_dex_action to take the new SwapAction

  - Update crates/policy-engine/src/pipeline.rs::Pipeline::evaluate:
    ```rust
    pub enum Verdict {
        Pass,
        Warn(Vec<PolicyId>),
        Fail(Vec<PolicyId>),
        Unsupported,  // NEW: returned for non-Swap actions
    }

    impl Pipeline {
        pub fn evaluate(&self, envelope: &ActionEnvelope, host: &HostSnapshot) -> Verdict {
            match &envelope.action {
                Action::Swap(swap) => {
                    let req = request_from_swap(swap, envelope.category, host);
                    self.engine.evaluate(req)
                }
                _ => Verdict::Unsupported,
            }
        }
    }
    ```

  Cedar DexContext compatibility:
    Old DexContext (from policy-schema/actions/dex.cedarschema):
      protocolIds: Set<String>
      inputTokens: Set<Token>
      outputTokens: Set<Token>
      totalInputUsd?: UsdValuation
      totalMinOutputUsd?: UsdValuation
      maxFeeBps?: Long
      hasZeroMinOutput: Bool
      hasExternalRecipient: Bool
      ...

    New SwapAction is single-hop. Lowering:
      protocolIds → derived from envelope category / adapter id (single element: e.g. ["uniswap-v2"])
      inputTokens → singleton: [token_in.address]
      outputTokens → singleton: [token_out.address]
      totalInputUsd → swap.enrichment.value_in_usd
      totalMinOutputUsd → swap.enrichment.min_value_out_usd
      maxFeeBps → swap.fee_bps
      hasZeroMinOutput → swap.amount_out.kind == Min && swap.amount_out.value == Some("0")
      hasExternalRecipient → swap.recipient != envelope.from (need access to root.from — passed through HostSnapshot or similar)

Tests:
  - test_lowering_swap_produces_correct_dex_context
  - test_lowering_non_swap_returns_none (each of 31 non-Swap variants)
  - test_pipeline_evaluate_returns_unsupported_for_permit (was Verdict::Pass; explicit Unsupported is clearer)

Commit: "refactor(policy-engine): swap-only lowering + Verdict::Unsupported (Phase 6.1)"
```

- [ ] **Verification + Review + Commit:** standard. Review focus: DexContext field-by-field lowering correctness, hasExternalRecipient edge case.

### Task 6.2: Delete signature policies + cedarschemas

**Files:**
- Delete: `policies/signature/` (entire dir)
- Delete: `policy-schema/actions/eip2612.cedarschema`
- Delete: `policy-schema/actions/eip712_other.cedarschema`
- Delete: `policy-schema/actions/other.cedarschema`
- Delete: `policy-schema/actions/permit2.cedarschema`
- Delete: `policy-schema/actions/signature_base.cedarschema`
- Modify: `policy-schema/` (master schema if any references the deleted schemas)
- Modify: Extension's embedded schema (`extension/.../default-policies/schema.cedarschema`)
- Modify: Integration test fixtures that referenced signature policies

- [ ] **Codex Delegation**

```
TASK: Delete obsolete signature policies and cedarschemas. Cedar schema now covers only swap.

Steps:
  1. git rm -r policies/signature
  2. git rm policy-schema/actions/{eip2612,eip712_other,other,permit2,signature_base}.cedarschema
  3. Keep policy-schema/actions/dex.cedarschema
  4. Keep policy-schema/core.cedarschema (base types: Wallet, Protocol, Token, UsdValuation, WindowStats)
  5. If there's a top-level policy-schema/schema.cedarschema that combines them, edit to reference only dex + core.
  6. Extension: update extension/<...>/default-policies/schema.cedarschema build script or static file to bundle only dex + core schemas.
  7. Any integration test that loaded a signature policy: either remove the test or convert it to assert Verdict::Unsupported.

Verification:
  cargo test --workspace
  - find policies -type f -name "*.cedar"  → only files under policies/dex
  - cd extension && npm run build  (will run in Phase 8; for now just check that the static schema file compiles)

Commit: "refactor: delete signature policies and cedarschemas (Phase 6.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 6.3: Remove LegacyAction

**Files:**
- Modify: `crates/policy-engine/src/core.rs` (remove `pub enum LegacyAction` + impls)
- Modify: `crates/policy-engine/src/lib.rs` (remove LegacyAction re-export)
- Modify: All call sites still referencing LegacyAction (should be 0 if Phase 5 wired everything to new Action)

- [ ] **Codex Delegation**

```
TASK: Delete LegacyAction. The codebase has been fully migrated to the 32-variant Action by Phase 5.

Steps:
  1. Find all references: git grep -nE '\bLegacyAction\b' -- 'crates/**/*.rs'
  2. The remaining references should be in:
     - crates/policy-engine/src/core.rs (the enum definition itself + impls)
     - crates/policy-engine/src/lib.rs (the re-export)
     If references exist ELSEWHERE: STOP — Phase 5 didn't fully migrate. Report to user.
  3. Delete the enum + all impls in core.rs
  4. Remove the re-export from lib.rs
  5. cargo build --workspace must pass

Tests: existing tests should still pass.

Verification:
  git grep -nE '\bLegacyAction\b' -- 'crates/**/*.rs'  → 0 matches
  cargo test --workspace

Commit: "refactor(policy-engine): remove LegacyAction (Phase 6.3)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 6 Review Gate

- [ ] All Task 6.1-6.3 통과
- [ ] `cargo test --workspace` green
- [ ] `git grep -nE '\bLegacyAction\b' -- 'crates/**/*.rs'` → 0 매치
- [ ] `policies/signature/` 존재 X, dex policies 10개 그대로
- [ ] `policy-schema/actions/` 에 `dex.cedarschema` 만 존재
- [ ] Cedar swap policy 10종 모두 통과/실패 테스트 동작 확인
- [ ] PR + 사용자 승인

---

# Phase 7 — web-server + WASM wire format 통일

**목적:** web-server `/api/decode` + `/api/sign` + WASM exports 모두 `request-router::route_request()` 단일 진입점 사용. HTTP integration tests 추가. **본 Phase 부터 wire format 변경 허용** (Phase 8 의 extension TS 와 atomic 으로 짝).

**브랜치:** `phase-7-unify-wire-format`

### Task 7.1: web-server endpoints use route_request

**Files:**
- Modify: `crates/web-server/src/main.rs` (또는 `src/routes/*.rs`)
- Modify: `crates/web-server/src/lib.rs`
- Modify: `crates/web-server/Cargo.toml` (request-router 의존성)

- [ ] **Codex Delegation**

```
TASK: web-server endpoints call request-router::route_request directly.

Files:
  - crates/web-server/src/main.rs (or src/routes/decode.rs + src/routes/sign.rs)

For POST /api/decode and POST /api/sign:
  1. Parse request body into (method, params, chain_id)
  2. Call request_router::route_request(&router_ctx, method, params, chain_id)
  3. Return result as JSON (RootRequest serialization)

RouterContext setup: web-server holds shared state with default registries built once at startup (lazy_static or AppState):
  - InMemoryDecoderRegistry (with V2/V3/V4/UR decoders)
  - InMemoryMapperRegistry (with V2/V3/V4/UR mappers)
  - InMemoryCallAdapterRegistry::from_decoder_registry(...)
  - InMemorySignAdapterRegistry (with eip2612 + permit2)
  - TokenRegistry impl (existing in web-server)

The old DecodeResponse / SignDecodeResponse types: replace with `RootRequest` direct serialization. This is a BREAKING change for HTTP clients — the extension (Phase 8) will be updated atomically.

Tests in this task: in-process integration tests (`crates/web-server/tests/http_integration.rs` new file):

```rust
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn decode_uniswap_v2_swap_returns_action_envelope() {
    let app = web_server::build_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/decode")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&json!({
                    "method": "eth_sendTransaction",
                    "params": [{"to": "0x7a25...", "data": "0x38ed1739..."}],
                    "chain_id": 1
                })).unwrap()))
                .unwrap(),
        ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000).await.unwrap()
    ).unwrap();
    assert_eq!(body["actions"][0]["action"], "swap");
}
```

Add 4-6 such tests covering swap, approve, permit2, eip2612.

Commit: "refactor(web-server): unify endpoints through request-router::route_request (Phase 7.1)"
```

- [ ] **Verification + Review + Commit:** standard. Review focus: HTTP integration test coverage; AppState lifecycle.

### Task 7.2: WASM exports use route_request

**Files:**
- Modify: `crates/policy_engine_wasm/src/lib.rs`

- [ ] **Codex Delegation**

```
TASK: WASM build_action_for_request_json now delegates to request-router::route_request.

File: crates/policy_engine_wasm/src/lib.rs

```rust
#[wasm_bindgen]
pub fn route_request_json(request_json: &str) -> Result<String, JsValue> {
    let req: RawRpcRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let registries = build_default_registries();
    let token_registry = StubTokenRegistry;
    let ctx = RouterContext {
        call_adapters: &registries.2,
        sign_adapters: &registries.3,
        decoder_registry: &registries.0,
        mapper_registry: &registries.1,
        token_registry: &token_registry,
        block_timestamp: req.block_timestamp,
    };

    let root = request_router::route_request(&ctx, &req.method, req.params, req.chain_id)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    serde_json::to_string(&root).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[derive(Deserialize)]
struct RawRpcRequest {
    method: String,
    params: serde_json::Value,
    chain_id: u64,
    block_timestamp: Option<u64>,
}
```

Keep the OLD exports (build_action_for_request_json) as #[deprecated] thin wrappers that internally call route_request_json — that's how the extension keeps working until Phase 8 cuts over. After Phase 8 commits, a follow-up commit in Phase 8 removes the deprecated wrappers.

Tests:
  - wasm-pack test --headless --chrome (existing)
  - Add: test_route_request_json_swap and test_route_request_json_permit

Commit: "feat(wasm): add route_request_json export (Phase 7.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 7.3: Capture post-refactor baseline + diff vs Phase 0 baseline

**Files:**
- Run script: `./scripts/capture_baseline.sh`
- Create: `crates/integration-tests/data/golden/baseline_post_refactor/`

- [ ] **Codex Delegation**

```
TASK: Capture post-refactor wire format outputs and produce diff report.

Steps:
  1. Modify scripts/capture_baseline.sh to dump into baseline_post_refactor/ instead.
  2. Run: ./scripts/capture_baseline.sh
  3. Generate diff report: scripts/diff_baselines.sh (new):

```bash
#!/usr/bin/env bash
set -euo pipefail
PRE=crates/integration-tests/data/golden/baseline_pre_refactor
POST=crates/integration-tests/data/golden/baseline_post_refactor
mkdir -p crates/integration-tests/data/golden/diff_reports

for f in "$PRE"/*.json; do
  name=$(basename "$f")
  diff -u "$f" "$POST/$name" > "crates/integration-tests/data/golden/diff_reports/$name.diff" || true
  echo "$name: $(wc -l < crates/integration-tests/data/golden/diff_reports/$name.diff) lines changed"
done
```

  4. Run diff_baselines.sh and commit the diff reports (so reviewers can see exactly what changed).

This is the CANONICAL wire format change record. The extension TS update in Phase 8 must match these diffs exactly.

Commit: "test: capture post-refactor baseline + diff report (Phase 7.3)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 7 Review Gate

- [ ] web-server HTTP integration tests pass
- [ ] WASM tests pass
- [ ] Phase 0 → Phase 7 wire-format diff is documented (`diff_reports/`)
- [ ] **HIGH RISK CHECKPOINT**: user reviews the diff reports before merging. Significant changes (e.g. field renamings) must be acknowledged.
- [ ] PR + 사용자 명시 승인 (mandatory; this Phase breaks wire format)

---

# Phase 8 — Extension TS 동기화

**목적:** extension 의 `wasm-bridge.types.ts` 를 새 32-variant `ActionEnvelope` 와 `RootRequest` 모양에 맞춰 재작성. vitest 통과 + chrome/firefox 빌드.

**브랜치:** `phase-8-extension-ts`

### Task 8.1: TS type definitions rewrite

**Files:**
- Modify: `extension/src/wasm-bridge.types.ts` (or wherever the types are)
- Modify: `extension/src/wasm-bridge.ts` (parser implementation)
- Create (optional): `extension/src/generated/action-types.ts` (codegen from Rust)

- [ ] **Codex Delegation**

```
TASK: Rewrite extension TS types to match the new 32-variant ActionEnvelope JSON wire format.

Files:
  - extension/src/wasm-bridge.types.ts (replace)
  - extension/src/wasm-bridge.ts (parser)

Approach options:
  A) Hand-write the TS types matching Rust definitions (faster, more brittle)
  B) Codegen from Rust using ts-rs or schemars + json-to-typescript (slower setup, more robust)

Recommendation: Option A for this PR; defer codegen automation to a future task.

TS types to add (mirroring Rust serde JSON):

```typescript
export type AssetKind = "native" | "erc20" | "erc721" | "erc1155" | "unknown";

export interface AssetRef {
  kind: AssetKind;
  chainId: number;
  address?: string;
  symbol?: string;
  decimals?: number;
}

export type AmountKind = "exact" | "min" | "max" | "unlimited" | "estimated" | "unknown";

export interface AmountConstraint {
  kind: AmountKind;
  value?: string;
}

export type ValiditySource = "tx-deadline" | "signature-deadline" | "grant-expiration";

export interface Validity {
  expiresAt: string;
  source: ValiditySource;
}

// ... and so on for all 32 action structs

export type Category = "dex" | "lending" | "rwa" | "liquid_staking" | "restaking" | "yield" | "misc" | "unknown";

export type Action =
  | { action: "swap"; fields: SwapAction }
  | { action: "approve"; fields: ApproveAction }
  | { action: "permit"; fields: PermitAction }
  | { action: "wrap"; fields: WrapAction }
  // ... all 32
  ;

export interface ActionEnvelope {
  category: Category;
  // flattened Action discriminator/payload:
  action: string;
  fields: any;  // typed via Action union
}

export interface RootRequest {
  schemaVersion: string;
  requestKind: "transaction" | "signature" | "userOperation";
  chainId: number;
  from: string;
  to: string;
  value: string;
  selector: string;
  protocol?: { name: string; version?: string; component?: string };
  actions: ActionEnvelope[];
  blockTimestamp?: number;
}
```

Parser (wasm-bridge.ts):
  - parseRootRequest(json: string): RootRequest with runtime shape validation
  - parseActionEnvelope(obj: unknown): ActionEnvelope
  - parseAction(obj: unknown): Action (discriminated union)
  - Use zod or hand-written validators for runtime safety

Update consumers within the extension (popup/background/content scripts) to handle the new shape.

The OLD types (DexAction/Permit2Action/Eip2612Action/...) are removed in the same diff.

Tests (vitest):
  - test_parse_swap_envelope (round-trip with sample JSON from baseline_post_refactor/)
  - test_parse_permit_envelope
  - test_parse_unknown_action_kind_throws

Commit: "feat(extension): rewrite TS types for new ActionEnvelope wire format (Phase 8.1)"
```

- [ ] **Verification**

```bash
cd extension
npm ci
npm run typecheck
npm test  # vitest
npm run build
```

- [ ] **Review:** code-reviewer on TS code. Focus: discriminated union exhaustiveness, runtime validation completeness, no untyped `any` leaks.

- [ ] **Commit:** Codex auto.

### Task 8.2: Remove deprecated WASM wrappers

**Files:**
- Modify: `crates/policy_engine_wasm/src/lib.rs`

- [ ] **Codex Delegation**

```
TASK: Remove the deprecated build_action_for_request_json wrapper. Extension is now on route_request_json only.

File: crates/policy_engine_wasm/src/lib.rs
  Delete the #[deprecated] wrappers added in Phase 7.2.

Verification:
  - cargo build -p policy-engine-wasm
  - extension build + tests still pass
  - cd extension && npm test && npm run build

Commit: "chore(wasm): remove deprecated build_action_for_request_json (Phase 8.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 8 Review Gate

- [ ] `cd extension && npm run typecheck` green
- [ ] `npm test` green
- [ ] `npm run build` green for both chrome and firefox manifests
- [ ] manual smoke test: load extension in Chrome dev mode, trigger a swap on Uniswap UI, verify popup shows correct decoded swap info
- [ ] PR + 사용자 명시 승인

---

# Phase 9 — CI 통합

**목적:** 새 통합 테스트, extension build, golden vector 회귀를 CI 에 추가.

**브랜치:** `phase-9-ci-integration`

### Task 9.1: web-server tests + golden vector regression in CI

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `crates/integration-tests/tests/golden_regression.rs`

- [ ] **Codex Delegation**

```
TASK: Add web-server tests and golden vector regression check to CI.

File: .github/workflows/ci.yml

Add jobs:
  - workspace-test: runs `cargo test --workspace --all-features`
  - clippy: `cargo clippy --workspace -- -D warnings`
  - web-server-http: spins up the server in-process via integration test (already covered if cargo test --workspace runs HTTP integration tests)
  - golden-regression: runs `cargo test -p integration-tests --test golden_regression`
  - fmt: `cargo fmt --all --check`
  - deny: `cargo deny check`

New test file: crates/integration-tests/tests/golden_regression.rs

```rust
//! Golden vector regression. For each input fixture, run route_request
//! and assert output matches the committed post-refactor baseline.

use std::fs;
use std::path::PathBuf;

#[test]
fn golden_vectors_match_baseline() {
    let root = env!("CARGO_MANIFEST_DIR");
    let inputs_dir = PathBuf::from(root).join("data/golden/inputs");
    let baseline_dir = PathBuf::from(root).join("data/golden/baseline_post_refactor");

    let registries = integration_tests::build_default_registries();
    let token_registry = integration_tests::StubTokenRegistry;

    for entry in fs::read_dir(&inputs_dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        let input: serde_json::Value = serde_json::from_reader(fs::File::open(&path).unwrap()).unwrap();
        let baseline: serde_json::Value = serde_json::from_reader(
            fs::File::open(baseline_dir.join(name)).unwrap()
        ).unwrap();

        let ctx = request_router::RouterContext { ... };
        let actual = request_router::route_request(
            &ctx,
            input["rpc"]["method"].as_str().unwrap(),
            input["rpc"]["params"].clone(),
            input["chain_id"].as_u64().unwrap(),
        ).unwrap();

        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            baseline,
            "fixture {} regressed",
            name,
        );
    }
}
```

Commit: "ci: add web-server HTTP + golden vector regression jobs (Phase 9.1)"
```

- [ ] **Verification + Review + Commit:** standard.

### Task 9.2: Extension build in CI

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Codex Delegation**

```
TASK: Add extension typecheck + build + test to CI.

In .github/workflows/ci.yml add a `extension` job:

```yaml
  extension:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - working-directory: extension
        run: |
          npm ci
          npm run typecheck
          npm test
          npm run build
```

If the extension build depends on a freshly-built WASM artifact, ensure the WASM build job runs first (use `needs:` directive).

Commit: "ci: add extension typecheck + build + test job (Phase 9.2)"
```

- [ ] **Verification + Review + Commit:** standard.

### Phase 9 Review Gate

- [ ] All Task 9.1-9.2 통과
- [ ] CI runs green on the Phase 9 PR
- [ ] golden_regression test passes locally + in CI
- [ ] PR + 사용자 명시 승인
- [ ] **MERGE feature/action-adapter-refactor → main** (final merge of the refactor program)

---

# Post-merge Cleanup

### Task Z.1: Remove capture_baseline binary

**Files:**
- Delete: `crates/integration-tests/src/bin/capture_baseline.rs`
- Modify: `crates/integration-tests/Cargo.toml`
- Delete: `scripts/capture_baseline.sh`, `scripts/diff_baselines.sh`

The post_refactor baseline 파일들 (`baseline_post_refactor/`) 은 golden_regression test 의 fixture 로 영구 유지. `baseline_pre_refactor/` + `diff_reports/` 는 후속 PR 에서 deleted (history 보존 위해 main 머지 후 별도 cleanup PR).

Commit: "chore: remove Phase 0 baseline capture tooling (cleanup)"

---

# Integration Checklist (전체 끝)

리팩토링이 main 에 머지된 후 다음을 한번에 확인:

- [ ] `cargo build --workspace` ✅
- [ ] `cargo test --workspace` ✅
- [ ] `cargo clippy --workspace -- -D warnings` ✅
- [ ] `cargo fmt --all --check` ✅
- [ ] `cargo deny check` ✅
- [ ] `cd extension && npm run build` ✅ (chrome + firefox)
- [ ] `cd extension && npm test` ✅
- [ ] `ls crates/adapters/` = `abi-resolver call-adapter mappers request-router sign-resolver` (정확히 5개) ✅
- [ ] `ls crates/adapters-bundle 2>&1` = ENOENT ✅
- [ ] `ls crates/abi-resolver 2>&1` = ENOENT ✅
- [ ] `git grep -nE '\bLegacyAction\b' -- 'crates/**/*.rs'` = 0 매치 ✅
- [ ] `git grep -nE 'use abi_resolver::Decoder|use mappers::Mapper' -- 'crates/adapters/request-router/**/*.rs'` = 0 매치 ✅
- [ ] `git grep -nE 'pub trait (CallAdapter|SignAdapter|Decoder|Mapper|CallAdapterRegistry|SignAdapterRegistry|DecoderRegistry|MapperRegistry)' -- 'crates/adapters/**/*.rs'` = 8 traits, 모두 trait 정의 ✅
- [ ] `policies/dex/` 10 cedar policy 모두 통과/실패 케이스 단위 테스트 ✅
- [ ] `policy-schema/actions/` = `dex.cedarschema` (단 1개) ✅
- [ ] CI green ✅
- [ ] manual extension smoke test (실제 Uniswap UI 에서 swap → popup → swap action 정상 표시) ✅

---

# 최종 위험 Recap

| Phase | 위험 | Mitigation |
|---|---|---|
| 1.9 | LegacyAction 이름 변경에서 import 경로 미스 → CI red | `git grep -wE 'Action(\W|$)'` 로 잔존 검출. Codex 가 cargo build 에러 보면서 반복 수정. |
| 1.5 | git mv 후 Cargo.toml path 오류 → workspace 빌드 깨짐 | path 변경 가짓수 적음 (4 crate × 의존 5-6개). 명시적으로 list 화. |
| 5 | request-router 가 Decoder/Mapper 직접 호출하는 코드 잔존 | Review Gate grep check 강제 |
| 6.1 | DexContext lowering 에서 single-hop → Set wrap 오류 → 정책 evaluation 결과 변경 | 기존 dex policy 10개의 단위 테스트가 동일 verdict 유지하는지 확인 (Phase 6 의 회귀) |
| 7-8 | wire format 변경이 atomic 하지 않으면 extension 깨짐 | Phase 7 의 #[deprecated] wrapper 가 Phase 8 commit 직전까지 호환 유지; Phase 8 의 cutover commit 에서 두 변경을 한 PR 에 묶기 |
| 8 | TS 수작업 typing 오류 | runtime parser (zod or hand-written) 로 schema mismatch 즉시 throw |
| 9 | golden_regression 이 oracle 가격 변동으로 fail | `host:oracle` 필드는 baseline 에 None 으로 캡쳐 — 테스트에서도 token_registry 만 stub 으로 사용 |

---

**Plan complete.**

Plan saved to `docs/plans/2026-05-13-action-and-adapter-refactor.md`.

Execution options:

1. **Subagent-Driven (recommended)** — orchestrator dispatches a fresh `codex:codex-rescue` agent per Task using the Codex Delegation block, runs Verification, dispatches `comprehensive-review:code-reviewer` for Review, asks user approval, commits, moves to next Task.

2. **Inline Execution** — Claude executes Tasks directly in this session using executing-plans, batching by Phase with user checkpoint at each Phase Review Gate.

**Recommendation: Subagent-Driven for this refactor.** The plan is 40+ tasks across 10 phases; fresh-context Codex per task avoids token bloat and gives clean per-task diffs for review.
