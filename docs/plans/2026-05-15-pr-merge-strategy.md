# Cross-PR Merge Strategy + Build Verification Matrix

Date: 2026-05-15
Audit scope: six open pull requests against `main` at `eb8ef88` ("policy-schema:forURCommand")
on `github.com/woojinnn/scopeball`.

Read-only audit. No merges, rebases, or pushes were performed against the audited
branches. Each branch was checked out into a disposable `/tmp/audit-pr<NN>`
worktree, built/tested, then cleaned up.

## 1. Per-PR status snapshot

| PR  | Branch                                        | HEAD sha    | Commits | Files (git diff) | Insertions | Deletions | Test result               |
| --- | --------------------------------------------- | ----------- | ------- | ---------------- | ---------- | --------- | ------------------------- |
| #20 | `chore/extension-deadcode-cleanup`            | `e3ebf36`   | 2       | 31               | 110        | 809       | green (73 vitest)         |
| #21 | `track-c-lending-lowering`                    | `66f444e`   | 8       | 44               | 1,952      | 387       | green (154 unit + 17 int) |
| #22 | `feat/adapter-registry-design`                | `72484b3`   | 2       | 23               | 1,096      | 356       | green (95 vitest)         |
| #23 | `track-d-adapter-registry-skeleton`           | `8c0fb43`   | 9       | 13               | 1,437      | 0         | green (15 vitest)         |
| #24 | `track-a-oracle-aggregator`                   | `9c323ba`   | 13      | 41               | 2,751      | 449       | green (69 vitest)         |
| #25 | `track-c-staking-restaking-lowering`          | `9660bf0`   | 10      | 49               | 2,951      | 387       | green (167 unit + 17 int) |

Notes:
- Commit counts above are `git rev-list --count origin/main..<branch>` — actual
  divergence from main.
- `gh pr view ... --json commits` truncates at 8 commits per PR, so PR #24
  appears to have 8 commits in the GitHub JSON; the branch actually has 13. PR
  #23 (9) and PR #25 (10) are also under the wire — the JSON view drops the
  oldest commits when the list exceeds 8. PR #24 has grown five commits since
  the task brief was written (`9c323ba`, `6662c4c`, `baf9577`, `33751fa`,
  `c7c2f34`, in addition to the originally-listed eight).
- File-touch counts from `git diff --name-only` include renames/deletes; the
  GitHub PR `files` list undercounts deletes (PR #21 shows 25 there, 44 here —
  the extra 19 are policy-schema files re-shaped by `66f444e`).
- "Tests" column = the most-relevant suite for that PR (see "Build verification
  matrix" below for the full list and commands).

### PR #20 — `chore/extension-deadcode-cleanup`
- Two-commit branch:
  - `32af84e` chore(extension): remove dead chains/ module + unused pending storage
  - `e3ebf36` fix(extension): restore "action" origin — silent regression in cleanup
- The second commit is a fix for a regression introduced by the first. Squash
  merge would erase that narrative (see "Squash strategy" below).
- Touches only `extension/`. No Rust, no schema.
- Removes `extension/src/background/chains/{chain-config,rpc-client}.ts`,
  `extension/src/background/storage.ts` pending-delta I/O, and the `chains/`
  unit tests.

### PR #21 — `track-c-lending-lowering`
- Eight feature commits implementing nine lending action variants (supply,
  withdraw, borrow, repay, liquidate, flash_loan, set_authorization,
  sign_authorization, revoke) plus dispatch wiring and the
  `LoweringError`/`Result` refactor.
- Final commit `66f444e fix(policy-schema): add validityDeltaSec to lending
  contexts` adds an optional `validityDeltaSec?: Long` field to four schemas
  (`supply`, `borrow`, `repay`, `sign_authorization`) and an end-to-end test in
  `crates/policy-engine/src/lowering/lending/supply.rs:134-187` that proves the
  lowering + schema + policy + engine pipeline closes.
- Branch HEAD `66f444e` builds clean on `wasm32-unknown-unknown`. 154
  policy-engine unit tests + 17 integration tests pass.

### PR #22 — `feat/adapter-registry-design`
- Two commits:
  - `c1ab71d` initial design spec + manifest TS types
  - `72484b3` review feedback: cross-field invariants, URL shape, `signature_alg`
- Adds:
  - `docs/specs/2026-05-15-adapter-registry-design.md` (211 lines)
  - `docs/plans/2026-05-15-tracks-a-and-c.md` (205 lines)
  - `extension/src/lib/adapter-manifest.ts` (321 lines) — canonical TS shape +
    `parseAdapterManifest()` parser
  - `extension/src/lib/__tests__/adapter-manifest.test.ts` (294 lines, 17 tests)
- No runtime wiring; pure types + parser. Net new lines, zero deletions.

### PR #23 — `track-d-adapter-registry-skeleton`
- Nine commits scaffolding `adapter-registry/` as an nginx-served static
  Docker container with a `scripts/build-manifest.js` generator and 15
  vitest-based shape tests.
- Crucially ships a **vendored copy** of PR #22's parser at
  `adapter-registry/tests/_vendored/adapter-manifest.ts` (256 lines).
- README at `adapter-registry/tests/README.md:25-37` already declares the
  follow-up plan: after PR #22 merges, delete `_vendored/` and import from
  `../../extension/src/lib/adapter-manifest.js`.

### PR #24 — `track-a-oracle-aggregator`
- 13 commits introducing a multi-source USD price aggregator under
  `policy-rpc/src/oracle/`:
  - `OracleSource` interface (`source.ts`)
  - Three sources: `chainlink.ts` (227 lines), `uniswap-v3-twap.ts` (283),
    `coingecko.ts` (143)
  - `aggregator.ts` (300 lines) coordinating them
  - `eth-provider.ts` viem-backed RPC layer (79 lines)
- 69 vitest tests pass, including v3-core reference vectors for the TickMath
  port and N=2-high-confidence anchor-stale behavior.
- Five tail commits (`33751fa`..`9c323ba`) are review/perf follow-ups:
  caching, error-code preservation, scaled-USD upscale coverage.

### PR #25 — `track-c-staking-restaking-lowering` (Track C2+C3)
- 10 commits. **Branched off PR #21's tip at `cbe6773a2`** (one commit
  *before* PR #21's HEAD `66f444e`). The first seven commits are byte-identical
  to PR #21's first seven. The last three add staking/restaking:
  - `2abca03` lower staking (stake, request_unstake, claim_unstake)
  - `23b06be` lower restaking (restake, request_restake_withdrawal,
    claim_restake_withdrawal)
  - `9660bf0` dispatch routing test extension
- 167 policy-engine unit tests + 17 integration tests pass.
- Inherits the entire PR #21 lending stack. Does **not** carry the `66f444e`
  validityDeltaSec schema fix — see "Cross-PR interactions" #2 below.

## 2. Cross-PR interactions

### 2a. PR #22 → PR #23 (canonical parser → vendored copy)
**Status: real divergence, post-merge follow-up required (already planned).**

Direct diff:
```
diff /tmp/audit-pr22/extension/src/lib/adapter-manifest.ts \
     /tmp/audit-pr23/adapter-registry/tests/_vendored/adapter-manifest.ts
```

The vendored copy in PR #23 is **not** byte-equal to the canonical parser in
PR #22. Differences the diff surfaces (the canonical PR #22 file is the
superset; PR #23 vendored is the subset):

| Feature                                                       | PR #22 canonical | PR #23 vendored |
| ------------------------------------------------------------- | ---------------- | --------------- |
| `signature_alg: string \| null` field on `AdapterVersion`     | yes              | **missing**     |
| `optionalNullableString` helper for `signature_alg` parsing   | yes              | **missing**     |
| Duplicate-`version` detection in `versions[]`                 | yes              | **missing**     |
| `url.startsWith("/")` registry-relative URL invariant         | yes              | **missing**     |
| `chain_id ∈ supported_chains[]` cross-field invariant         | yes              | **missing**     |

The header comment of the vendored file
(`/tmp/audit-pr23/adapter-registry/tests/_vendored/adapter-manifest.ts:1-14`)
explicitly says this is a temporary stub pending the canonical module landing.
`adapter-registry/tests/README.md` enumerates the post-merge fix: replace the
vendored copy with an import of `../../extension/src/lib/adapter-manifest.js`.

**Impact on merge order:** PR #22 must land before PR #23's `_vendored/`
cleanup commit, but PR #23 itself can land before or after PR #22 because the
two PRs do not touch overlapping files (PR #22 adds `extension/src/lib/`, PR #23
adds `adapter-registry/`). A follow-up commit/PR on `track-d-adapter-registry-skeleton`
is needed once PR #22 is merged.

### 2b. Wire-format coupling: `signature_alg` (PR #22 ↔ PR #23 generator)
**Status: forward-compatible by design. No action needed.**

PR #23's `adapter-registry/scripts/build-manifest.js:228-241` emits per-version
records that do **not** include `signature_alg`. PR #22's canonical parser
treats the field via `optionalNullableString`
(`extension/src/lib/adapter-manifest.ts:200-204`), which returns `null` when
the key is absent:

```
/private/tmp/audit-pr22/extension/src/lib/adapter-manifest.ts:290-295
function optionalNullableString(
  record: JsonRecord,
  key: string,
  path: string,
): string | null {
  if (!Object.prototype.hasOwnProperty.call(record, key)) return null;
```

Verified end-to-end by running the empty manifest from `node
adapter-registry/scripts/build-manifest.js` and a fabricated populated one
through `parseAdapterManifest` — both parse cleanly with `signature_alg: null`.

### 2c. PR #23 generator emits `display_name_override` — stripped before output
PR #23 `scripts/build-manifest.js:240` produces `display_name_override` on each
version object, but line 306
(`versions.map(({ display_name_override, ...rest }) => rest)`) destructures it
out before the JSON is emitted. The on-wire shape matches PR #22's parser.
Worth noting because reading the version-builder in isolation looks like a
schema-drift bug; it is not.

### 2d. PR #21 → PR #25 (lending stack ↔ staking/restaking)
**Status: branch lineage problem. PR #25 will need a rebase before merging.**

PR #25 was cut from PR #21 at sha `cbe6773a` (commit 7 of PR #21's 8), so the
final PR #21 commit `66f444e` (validityDeltaSec schema fix) is **missing** from
the PR #25 tree. Two consequences:

1. **Missing schema fix:** None of the four `policy-schema/actions/lending/*.cedarschema`
   files in PR #25 contain the `validityDeltaSec?: Long` field. Verified:
   ```
   grep -n "validityDeltaSec" /tmp/audit-pr25/policy-schema/actions/lending/*.cedarschema
   # (no matches)
   ```
   In contrast, all four PR #21 schemas have the field at line 12 or 15.

2. **Missing test:** The `supply_policy_referencing_validity_delta_sec_evaluates_end_to_end`
   test in `crates/policy-engine/src/lowering/lending/supply.rs:134-187` is
   present in PR #21 (188 lines) and absent in PR #25 (134 lines).

3. **`dispatch.rs` arm reshuffle (clean under rebase-merge):** PR #21 lists
   the staking variants under `Err(LoweringError::UnsupportedAction)`
   (`/tmp/audit-pr21/crates/policy-engine/src/lowering/dispatch.rs:102-108`).
   PR #25 deletes those `Action::Stake(_)` lines from the error arm and adds
   matching `Ok(action.build(&ctx))` lines higher up
   (`/tmp/audit-pr25/.../dispatch.rs:93-98`). Important: PR #21's tail commit
   `66f444e` does **not** touch `dispatch.rs` — it only modifies four
   `.cedarschema` files and adds a test in `supply.rs`. So PR #25's three
   unique commits (authored against `cbe6773` = PR #21's commit 7) will replay
   cleanly onto a `dispatch.rs` baseline they already expect. **Under rebase-merge
   of PR #21 the rebase is clean; under squash-merge the seven duplicate
   commits land with new SHAs and a manual `git rebase --onto main cbe6773
   track-c-staking-restaking-lowering` is the safer mechanic** (or rely on
   `git rebase`'s patch-id de-duplication).

**Required action when PR #21 lands:**
- Rebase PR #25 onto the post-merge `main` (use `--onto` if PR #21 was
  squash-merged).
- Verify the rebased `dispatch.rs` arm list compiles and the routing test
  still passes (it's PR #25's `9660bf0`).
- After rebase, the engine builder for staking schemas needs to be re-checked
  if/when `validityDeltaSec` is also expected to apply to staking contexts
  (out of scope for this audit).

### 2e. PR #20 squash strategy
PR #20 has two commits where commit 2 fixes a regression introduced by commit
1:
- `32af84e` cleanup that accidentally removed the "action" origin tag
- `e3ebf36` fix that restores it (added a new vitest at
  `extension/src/background/__tests__/wasm-bridge.test.ts` to guard against
  re-introducing the regression)

**Recommendation: rebase-and-merge OR craft a combined squash message** that
calls out both halves. A plain squash with the PR-title message loses the
regression-after-cleanup narrative, which is exactly the institutional memory
that protects the next person doing dead-code surgery on `orchestrator.ts`.

### 2f. PR #24 has no overlap with any other open PR
PR #24 touches only `policy-rpc/` and a couple of root files
(`policy-rpc/Dockerfile`, `policy-rpc/package-lock.json`,
`policy-rpc/package.json`, `policy-rpc/README.md`). No collision with PR
#20 (extension only), PR #21/#25 (Rust + policy-schema), PR #22/#23
(adapter-registry + extension lib).

## 3. Recommended merge order

The dependency graph:

```
PR #22 (design + canonical TS) ──┐
                                 ├──► PR #23 follow-up (delete _vendored/, re-import)
PR #23 (registry skeleton) ──────┘

PR #21 (lending) ──► PR #25 rebase ──► PR #25 merge

PR #20  (independent)
PR #24  (independent)
```

Suggested chronological order:

1. **PR #20** (`chore/extension-deadcode-cleanup`) — smallest blast radius, no
   dependencies, deletes are easy to revert if needed. Use **rebase-merge**
   (not squash) to preserve the cleanup→fix story.
2. **PR #22** (`feat/adapter-registry-design`) — no runtime impact, pure
   types/docs. Unblocks PR #23 follow-up cleanup.
3. **PR #24** (`track-a-oracle-aggregator`) — independent of all others, all 69
   tests pass. Can land in parallel with PR #22.
4. **PR #23** (`track-d-adapter-registry-skeleton`) — can land any time after
   or before PR #22; lands its `_vendored/` cleanup as a follow-up commit on
   `track-d-adapter-registry-skeleton` (or a new branch) after PR #22 is on
   `main`.
5. **PR #21** (`track-c-lending-lowering`) — the larger Rust PR. Lands
   independently. Its schema fix `66f444e` is the bridge for PR #25.
6. **PR #25** (`track-c-staking-restaking-lowering`) — rebase onto post-PR-#21
   `main`, verify the `dispatch.rs` arm reshuffle replays cleanly (it should,
   since PR #21's tail commit doesn't touch dispatch.rs), then merge.

Rationale:
- PR #20 first: pure deletion that lowers cognitive load on subsequent
  conflict reviews. No interaction with anyone else.
- PR #22 before PR #23 to enable the `_vendored/` cleanup commit, even though
  technically PR #23 doesn't require it.
- PR #24 anywhere — no interaction.
- PR #21 before PR #25 is mandatory; PR #25 carries seven of PR #21's commits
  and depends on the eighth (`66f444e`).
- PR #23 and PR #25 are the only ones with required follow-up work after their
  upstream dependency lands.

## 4. Build verification matrix

Each branch was checked out into a fresh `/tmp/audit-pr<NN>` worktree via
`git worktree add --detach`. Cargo target dirs were isolated per branch
(`CARGO_TARGET_DIR=/tmp/audit-target-pr<NN>`) to avoid build cache collisions.

| Check                                                  | PR #20 | PR #21 | PR #22 | PR #23 | PR #24 | PR #25 |
| ------------------------------------------------------ | ------ | ------ | ------ | ------ | ------ | ------ |
| `cargo build -p policy-engine`                         | n/a    | pass   | n/a    | n/a    | n/a    | pass   |
| `cargo build -p policy-engine-wasm --target wasm32`    | n/a    | pass   | n/a    | n/a    | n/a    | pass   |
| `cargo test -p policy-engine`                          | n/a    | 154 ok | n/a    | n/a    | n/a    | 167 ok |
| `cargo test -p policy-engine-integration-tests`        | n/a    | 17 ok  | n/a    | n/a    | n/a    | 17 ok  |
| `scripts/wasm-build.sh` (wasm artifact for extension)  | pass   | n/a    | pass¹  | n/a    | n/a    | n/a    |
| extension `npx vitest run`                             | 73 ok  | n/a    | 95 ok  | n/a    | n/a    | n/a    |
| extension `yarn build:chrome`                          | pass   | n/a    | pass   | n/a    | n/a    | n/a    |
| adapter-registry `npx vitest run`                      | n/a    | n/a    | n/a    | 15 ok  | n/a    | n/a    |
| adapter-registry `node scripts/build-manifest.js`      | n/a    | n/a    | n/a    | pass   | n/a    | n/a    |
| policy-rpc `npx vitest run`                            | n/a    | n/a    | n/a    | n/a    | 69 ok  | n/a    |

¹ For PR #22, the extension vitest suite requires the wasm artifact
(`extension/src/wasm/`) because the `wasm-bridge.test.ts` ships in main and is
not stubbed. The artifact was copied from the main worktree build to avoid
re-running wasm-pack. PR #22 doesn't touch any Rust code so the artifact is
binary-equivalent to one built on its own branch.

### Baseline verification (`origin/main`)
- `scripts/wasm-build.sh` → green (wasm-pack 0.13.1, ~63s cold)
- `extension/ yarn install && npx vitest run` → 78 tests pass, 12 files
- This matters: a vitest fail attributable to a missing wasm artifact on a
  fresh checkout is **a pre-existing repo state** — the wasm artifact is not
  committed to git and must be built first. Every extension PR audit step
  built or copied this artifact first.

### Failure / red rows: none

All audited branches build and test green. No first-20-lines of failure to
capture.

### Notes on perf
- Cargo cold compile of the workspace runs ~17s per branch on this host with
  isolated target dirs, plus ~24s for `--target wasm32-unknown-unknown`.
- `wasm-pack build --release` is the long pole (~63s cold) and is needed for
  every extension PR audit.
- The vitest runs are sub-second after install.

## 5. Reconciliation findings (cross-PR conflicts)

### Confirmed real conflicts

1. **PR #23 vendored parser is out-of-sync with PR #22 canonical** — five
   missing features listed in §2a. Already documented in PR #23's README;
   follow-up commit/PR required after PR #22 merges.

2. **PR #25 missing the PR #21 tail commit (`66f444e`)** — both the four
   schema files and the supply.rs e2e test
   (`crates/policy-engine/src/lowering/lending/supply.rs:134-187`) are absent
   from PR #25. Rebase required when PR #21 lands.

3. **PR #25 `dispatch.rs` arm reshuffle (replays cleanly)** — PR #21 marks the
   six staking variants as `UnsupportedAction`; PR #25 reclassifies them as
   supported. Same file, same match block. Crucially PR #21's tail commit
   `66f444e` does not touch `dispatch.rs`, so PR #25's three unique commits
   (authored against PR #21's commit-7 baseline) will rebase cleanly onto
   post-PR-#21 main. Under squash-merge, prefer `git rebase --onto`. No human
   conflict resolution expected — but `cargo test -p policy-engine` should be
   re-run on the rebased branch as a sanity check.

### Confirmed non-issues

4. **`signature_alg` wire-format gap** — PR #23 doesn't emit it but PR #22
   parses with `optionalNullableString` (absent → null). Forward-compatible by
   design (PR #22 doc-comment line 287 says so). Verified with two probe tests
   (empty + populated manifest) against PR #22's parser. **No action needed.**

5. **PR #23 `display_name_override` per-version field** — generated internally
   but stripped at JSON-emit (`scripts/build-manifest.js:306`). On-wire shape
   matches the canonical parser. **No action needed.**

6. **PR #20 — no cross-PR conflicts** — deletes are confined to
   `extension/src/background/chains/`, which no other PR touches.

7. **PR #24 — no cross-PR conflicts** — confined to `policy-rpc/`.

## 6. Artifacts & cleanup

Disposable worktrees used during this audit (cleaned up after the doc lands):
- `/tmp/audit-pr20` @ `e3ebf36`
- `/tmp/audit-pr21` @ `66f444e`
- `/tmp/audit-pr22` @ `72484b3`
- `/tmp/audit-pr23` @ `8c0fb43`
- `/tmp/audit-pr24` @ `9c323ba` (refreshed from `5a8e049` after spotting PR #24
  had grown five commits beyond the GitHub JSON view)
- `/tmp/audit-pr25` @ `9660bf0`
- `/tmp/audit-main` @ `eb8ef88` (for baseline wasm build only)

Cargo target dirs:
- `/tmp/audit-target-pr21`, `/tmp/audit-target-pr25`

These are removed after audit completion.

## 7. Quick reference: file paths cited

- `/tmp/audit-pr22/extension/src/lib/adapter-manifest.ts` (canonical types + parser)
- `/tmp/audit-pr22/extension/src/lib/__tests__/adapter-manifest.test.ts` (17 tests)
- `/tmp/audit-pr23/adapter-registry/scripts/build-manifest.js` (generator)
- `/tmp/audit-pr23/adapter-registry/tests/_vendored/adapter-manifest.ts` (vendored copy)
- `/tmp/audit-pr23/adapter-registry/tests/README.md` (reconciliation plan)
- `/tmp/audit-pr21/crates/policy-engine/src/lowering/lending/supply.rs:134-187` (validity-delta-sec e2e test)
- `/tmp/audit-pr21/policy-schema/actions/lending/{supply,borrow,repay,sign_authorization}.cedarschema` (validityDeltaSec field)
- `/tmp/audit-pr21/crates/policy-engine/src/lowering/dispatch.rs:102-108` (staking marked unsupported)
- `/tmp/audit-pr25/crates/policy-engine/src/lowering/dispatch.rs:93-98` (staking marked supported)
- `/tmp/audit-pr25/crates/policy-engine/src/lowering/staking/` (new module)
- `/tmp/audit-pr25/crates/policy-engine/src/lowering/restaking/` (new module)
- `/tmp/audit-pr24/policy-rpc/src/oracle/aggregator.ts` (multi-source aggregator)
- `/tmp/audit-pr24/policy-rpc/src/oracle/sources/{chainlink,uniswap-v3-twap,coingecko}.ts`
- `/tmp/audit-pr20/extension/src/background/__tests__/wasm-bridge.test.ts` (regression guard)
