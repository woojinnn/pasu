# Phase 8 Aerodrome — Audit Findings (Round 7)

**Audit date**: 2026-05-19
**Auditor**: `solidity-auditor` sub-agent (read-only, A-F-AUDIT)
**Worktree**: `phase8-aerodrome`
**Scope**: ScopeBall PoC infrastructure (Rust + JSON DSL + Cedar policy), NOT on-chain Aerodrome contracts
**Methodology**: QuillAudits BSA framing (Behavioral Decomposition → Threat Modeling → Adversarial Simulation → Risk Scoring)

## Summary

| Severity | Count | Status |
|---|---|---|
| **P0** | 1 | ✅ Fixed (Round 7b) |
| **P1** | 6 | 4 fixed (Round 7b), 1 false-positive (verified scalar), 3 forward-spec |
| **P2** | 4 | 0 fixed, 1 verified, 3 forward-spec / noted |
| **P3** | 3 | 0 fixed, 3 noted (defensive programming) |
| **Total** | 14 | 5 closed, 9 open (mostly host: capability dependencies) |

**Confidence**: high for P0/P1 (grep + read verified). Medium for #11 (resolved via upstream contract verify).

## Severity Rubric

- **P0 (Critical)**: Fund loss / unauthorized privileged access / permanent DoS / fail-open (default verdict pass when should forbid)
- **P1 (High)**: Authentication bypass / governance takeover / conditional fund loss / silent decode failure / missing validation widening attack surface
- **P2 (Medium)**: Limited fund loss / unexpected state / recoverable DoS / over-trust of input
- **P3 (Low)**: Edge case / informational leakage / minor deviation from spec / best-practice deviation

---

## Findings

### ✅ #1 — P0 — ClaimRewards lowering missing (fail-open)

**Status**: **FIXED** (Round 7b, commit `3442a43`)

**Area**: 1 (lowering)
**File**: `crates/policy-engine/src/lowering/dispatch.rs:81-106`

**Issue**: `Action::ClaimRewards(_)` was not present in the lowering dispatch match, so it fell through to `_ => Ok(None)` (silently skipped). Verified via `evaluate_envelopes_inner` at line 708-712.

**Impact**: All 8 Aerodrome reward/fee/bribe envelopes + Slipstream NPM `collect` envelope produced zero `PolicyRequest`. **Cedar policies on `claim_rewards` were dead code; every reward-claim tx failed-open with verdict `pass`.**

**Fix applied**:
- NEW `crates/policy-engine/src/lowering/misc/claim_rewards.rs` (Source / From / Recipient / Rewards Set)
- `lowering/dispatch.rs`: `Action::ClaimRewards(action) => action.build(&ctx).map(Some)` arm added
- `context_keys.rs`: `SOURCE_ADDRESS` / `SOURCE_LABEL` / `REWARDS` consts added
- NEW `policy-examples/aerodrome/forbid-claim-rewards-unknown-source.cedar` (defense-in-depth: forbid when `context.sourceAddress` is empty)
- 3 unit tests added

---

### ✅ #2 — P1 — `build_lock_increase_envelope` missing kind-required-field enforcement

**Status**: **FIXED** (Round 7b)

**Area**: 2 (builder)
**File**: `crates/adapters/mappers/src/declarative/single_emit.rs:1212-1230`

**Issue**: Did not enforce `kind == Amount ⇒ additionalAmount.is_some()` nor `kind == UnlockTime ⇒ newLockDurationSec.is_some()`. Schema fields are `Option`.

**Impact**: A malicious or buggy manifest could emit `LockIncrease { kind: Amount, additional_amount: None }`. Cedar policy cannot evaluate the amount (`context has additionalAmount` is false). User signs an unspecified lock-amount change without policy gate.

**Fix applied**: Per-kind required-field enforcement (`match kind { Amount => additional_amount.ok_or(...)?, UnlockTime => new_lock_duration_sec.ok_or(...)? }`) + 2 unit tests.

---

### ✅ #3 — P1 — `build_lock_manage_envelope` missing kind-required-field enforcement

**Status**: **FIXED** (Round 7b)

**Area**: 2 (builder)
**File**: `crates/adapters/mappers/src/declarative/single_emit.rs:1232-1250`

**Issue**: Did not enforce `kind == Merge ⇒ to_token_id.is_some()` nor `kind == Split ⇒ split_ratio.is_some()`.

**Impact**: Same fail-open as #2. The `forbid-self-merge.cedar` policy explicitly requires `context has toTokenId` and silently passes if absent — a manifest missing `toTokenId` bypassed the merge integrity check.

**Fix applied**: Per-kind required-field enforcement + 2 unit tests.

---

### ✅ #4 — P1 — `build_gauge_vote_envelope` missing kind=Reset/Poke enforcement

**Status**: **FIXED** (Round 7b)

**Area**: 2 (builder)
**File**: `crates/adapters/mappers/src/declarative/single_emit.rs:1125-1154`

**Issue**: Did not enforce `kind == Reset ⇒ pools.is_empty() && weights.is_empty()`. The Cedar policy `forbid-zero-weight-sum` waives the weight-sum check when `kind == "reset"`.

**Impact**: A manifest binding `kind: { literal: "reset" }` to a real `vote()` selector (typo or malice) lets gauge_vote with non-zero pools bypass the weight-sum policy entirely.

**Fix applied**:
```rust
match kind {
    Some(GaugeVoteKind::Reset) | Some(GaugeVoteKind::Poke) => {
        if !pools.is_empty() || !weights.is_empty() {
            return Err(MapperError::Internal(...));
        }
    }
    _ => {}
}
```
+ 2 unit tests.

---

### ⏳ #5 — P1 — Gauge bundles `lpToken.address = $.tx.to` placeholder

**Status**: **FORWARD-SPEC** (requires `host:lp_token_resolver` capability design)

**Area**: 4 (bundles)
**Files**:
- `registry/manifests/aerodrome/gauge/deposit@1.0.0.json:48`
- `registry/manifests/aerodrome/gauge/withdraw@1.0.0.json:48`

**Issue**: `lpToken.address = $.tx.to` — the gauge contract address is reported as the LP token. The LP token is a separate ERC20 (the pool token).

**Impact**: Cedar policies allow-listing LP tokens by address never match the real LP token; deny-list policies cannot reject malicious LP. `LpStake/LpUnstake` Cedar policy `context.lpToken.address` always equals the gauge contract.

**Recommendation**:
- (a) Literal pool address per gauge `to[]` entry — requires per-pool manifest (verbose, 20+ V2 + 15+ CL gauges)
- (b) Add `host:lp_token_resolver` capability

Cleanest fix = (a) for top gauges, (b) for full coverage. Phase 8 chose 단순 placeholder + `host:token_metadata` Phase 7 enrichment 의 의 의 의 (limitation 명시).

---

### ⏳ #6 — P1 — V2 add/remove + Slipstream NPM mint `pool.address = 0x0...0` placeholder

**Status**: **FORWARD-SPEC** (requires `host:pool_address_compute` capability)

**Area**: 4 (bundles)
**Files**:
- `registry/manifests/aerodrome/v2/addLiquidity@1.0.0.json:33`
- `registry/manifests/aerodrome/v2/removeLiquidityETH*@1.0.0.json:32`
- `registry/manifests/aerodrome/slipstream-npm/mint@1.0.0.json:42`

**Issue**: Pool address is computable from tokens + stable flag (Aerodrome V2) or token0/token1/tickSpacing (Slipstream) but not emitted — `0x0000...0000` placeholder.

**Impact**: Cedar policies on `pool.address` allow/deny-list silently inapplicable — every V2 add/remove and Slipstream mint envelope claims the zero pool.

**Recommendation**:
- (a) Compute pool address via `host:pool_address_compute` capability (`computePoolAddress(token0, token1, stable, factory)` or CREATE2 derivation)
- (b) Omit the field entirely + update schema to `pool.address?` with Cedar policy guard pattern

---

### ⏳ #7 — P1 — Voter `claimBribes`/`claimFees` rewardTokens first-element fan-out

**Status**: **FORWARD-SPEC** (requires opcode-stream over array or `host:multi_token_expansion`)

**Area**: 4 (bundles)
**Files**:
- `registry/manifests/aerodrome/voter/claimBribes@1.0.0.json:33`
- `registry/manifests/aerodrome/voter/claimFees@1.0.0.json:33`

**Issue**: `rewardTokens[0].address = $.args.tokens[0][0]` only — captures **first nested token of first pool**. Drops every other reward token.

**Impact**: Cedar policies on `rewardTokens[N].address` see at most one token; multi-token / multi-pool bribe claims are unobservable. User may sign a claim that drains an attacker-injected token while policy only sees the first.

**Recommendation**:
- (a) Re-emit as opcode-stream over `$.args.tokens[*][*]` (DSL extension)
- (b) Add `host:multi_token_expansion` capability
- (c) Defensive default policy: `forbid claim_rewards when context.rewardTokens.len() > 1` (defense-in-depth until proper fix)

---

### ⏳ #8 — P2 — `set_nested` `[N]` segment unbounded index

**Status**: **NOTED** (defense-in-depth — gated by SHA-256 registry verify)

**Area**: 2 (builder)
**File**: `crates/adapters/mappers/src/declarative/single_emit.rs:202-213` (`set_nested` Array step) + line 275 (`parse_segment` uses `usize::parse()`)

**Issue**: `while arr.len() <= *idx { arr.push(Value::Null); }` — no upper bound on `[N]` parsed by `parse_segment`.

**Impact**: A malicious manifest with `field[1000000000]` triggers a 1B-element vec push loop → WASM OOM / DoS. Gated by SHA-256 registry verification, but defense-in-depth is missing.

**Recommendation**: Add `const MAX_FIELD_ARRAY_INDEX: usize = 64;` check in `parse_segment`, reject larger indices with `MapperError::Internal`.

**Note**: Round 5a 의 `set_nested [N]` extension fix 의 의 의 phase 7 latent bug 의 fix. 추가 hardening = 본 finding 의 의 의 의 추후 의 의 의 추가.

---

### ⏳ #9 — P2 — Slipstream NPM increase/decreaseLiquidity asset.address missing

**Status**: **FORWARD-SPEC** (requires `host:nfpm_position_resolver` capability)

**Area**: 4 (bundles)
**Files**:
- `registry/manifests/aerodrome/slipstream-npm/decreaseLiquidity@1.0.0.json`
- `registry/manifests/aerodrome/slipstream-npm/increaseLiquidity@1.0.0.json`

**Issue**: `inputTokens[N].asset.kind = "erc20"` declared without `asset.address`. NFPM positions require an RPC lookup to resolve token0/token1.

**Impact**: Cedar policies on `inputTokens/outputTokens.asset.address` cannot allow-list pool tokens — every NPM IL/DL operation looks identical to policy engine. Token-specific spending limits silently inapplicable.

**Recommendation**:
- (a) Omit `asset.kind` (no claim)
- (b) Add `host:nfpm_position_resolver` enrichment via `host_capabilities`

---

### ⏳ #10 — P2 — `declarative_lookup_json` no WasmChildResolver wire-up

**Status**: **FORWARD-SPEC** (Phase 7 limitation #2, identical scope)

**Area**: 5 (cross-cutting)
**File**: `crates/policy-engine-wasm/src/declarative_exports.rs:326-336`

**Issue**: `declarative_lookup_json` builds `MapContext { resolver: None }`. Aerodrome `slipstream-npm/multicall` bundle uses `multicall_recurse` strategy → faults with "multicall_recurse requires ctx.resolver".

**Impact**: Every NPM operation wrapped in `multicall()` (the standard frontend pattern) faults declaratively → falls through to static path. Phase 8 NFPM observability gap: the very bundles Phase 8 added (mint/burn/IL/DL/collect) are unreachable via multicall.

**Recommendation**: Wire up `WasmChildResolver` in `declarative_lookup_json` per Phase 7 limitation #2 (`scopeball/CLAUDE.md`), or explicitly document and surface a `unsupported_strategy_via_wasm` error to the caller.

**Note**: Phase 7 limitation 와 동일 — Phase 8 의 의 의 의 의 introduce 의 의 의 의 (의 의 surface 확장 만).

---

### ✅ #11 — P1 → RESOLVED — VotingEscrow.split ABI

**Status**: **VERIFIED CORRECT** (audit assumption was wrong)

**Area**: 4 (bundles)
**File**: `registry/manifests/aerodrome/voting-escrow/split@1.0.0.json:17`

**Audit claim (unverified)**: ABI declares `ratios` as `uint256` — but actual `VotingEscrow.split` takes `uint256[] amounts`.

**Verification (Round 7b)**: Fetched `https://raw.githubusercontent.com/aerodrome-finance/contracts/main/contracts/VotingEscrow.sol`. Actual signature: `function split(uint256 _from, uint256 _amount) external nonReentrant returns (uint256, uint256)` — **scalar uint256, NOT array**. Selector `0x4b19becc` verified via `cast sig "split(uint256,uint256)"`.

**Conclusion**: Bundle is correct as-is. Cosmetic param name mismatch (`ratios` vs upstream `_amount`) is functionally inconsequential (serde positional decode).

---

### ⏳ #12 — P3 — gauge_vote lowering silent skip of malformed weights

**Status**: **NOTED** (defensive programming improvement)

**Area**: 1 (lowering)
**File**: `crates/policy-engine/src/lowering/misc/gauge_vote.rs:48-53`

**Issue**: Saturating sum silently skips malformed weight strings (`if let Ok(parsed) = U256::from_str_radix`).

**Impact**: Malformed manifest emitting non-decimal weights yields `weightsSum == "0"` and triggers `forbid-zero-weight-sum` falsely — masking real bug as policy-deny.

**Recommendation**: Either propagate parse error (`?`) or document the saturating-on-error semantics in a doc comment.

**Note**: Practically theoretical — ABI `uint256[]` cannot encode malformed strings; only adversarial DSL manifest construction could trigger.

---

### ⏳ #13 — P3 — `expect("...")` panics in path decoders

**Status**: **NOTED** (defensive programming improvement)

**Area**: 3 (BuiltinFn)
**File**: `crates/adapters/mappers/src/declarative/builtin_fn.rs:121,127,133,139,177,183,189,197`

**Issue**: `expect("decode_v3_path guarantees ...")` / `expect("decode_slipstream_path guarantees ...")` panics — invariant relies on the decoder's success contract.

**Impact**: If a future refactor weakens decoder invariants, WASM panics rather than returning `MapperError`. Currently sound but brittle.

**Recommendation**: Replace `expect(...)` with `.ok_or(FnError::PathDecode { ... })?` for defense-in-depth.

---

### ⏳ #14 — P3 — No programmatic address verify against upstream

**Status**: **NOTED** (process improvement — verify script)

**Area**: 4 (bundles)
**Files**: All Aerodrome bundles `match.to[]`

**Issue**: Addresses use EIP-55 checksum casing. `bundle-schema.ts:207` regex accepts both cases (correct), but no programmatic validation against Aerodrome's deploy-addresses repo.

**Impact**: A typo in an address (e.g. `0xeBf418Fe...6B4` vs real on-chain address) silently lands in bridge table and never matches — bundle becomes dead code.

**Recommendation**: Add `npm run verify-addresses` script that cross-checks all `match.to[]` against `https://github.com/aerodrome-finance/contracts` `DeployCore-Base.json`.

---

## Forward-spec roadmap

| Finding | Capability needed | Estimated effort |
|---|---|---|
| #5 | `host:lp_token_resolver` | 0.5 day (per-pool manifest) OR 1 day (host capability) |
| #6 | `host:pool_address_compute` | 1 day (CREATE2 derivation in client) |
| #7 | DSL opcode-stream over array OR `host:multi_token_expansion` | 1-2 days |
| #8 | Bounded `[N]` parse (mechanical) | 0.25 day |
| #9 | `host:nfpm_position_resolver` | 1 day |
| #10 | `WasmChildResolver` wire-up in `declarative_lookup_json` | 0.5 day |
| #12, #13 | Defensive programming refactor | 0.25 day each |
| #14 | `npm run verify-addresses` script | 0.25 day |

## References

- Audit methodology: `~/.claude-web3/CLAUDE.md` (UPSide Academy security researcher profile)
- Plan: `~/.claude-web3/plans/zany-leaping-alpaca.md` §1.4 (critical findings) + §13.1 (Round 7a/7b log)
- Spec: `ScopeBall/ADAPTER_MARKETPLACE_ARCHITECTURE.md`
- Aerodrome upstream: https://github.com/aerodrome-finance/contracts (verified for #11)
