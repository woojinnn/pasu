# Compound V3 Protocol Onboarding Evidence

> Revalidation run for existing `compound-v3` artifacts. Existing manifests and
> corpus entries were treated as candidates, then gated with inventory,
> synthetic fuzz, strict corpus pins, and final workspace checks.

## Run Metadata

| field | value |
|---|---|
| protocol | compound-v3 |
| branch | feat/Compound-onboarding |
| worktree | /Users/woojin/Desktop/upside_academy/project/policy-engine/.claude/worktrees/Compound-onboarding |
| date | 2026-06-01 |
| main agent | Codex GPT-5 |
| base commit | 2e3c3297 |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Codex current-session research executed | done | Codex read `crates/integration-tests/ONBOARDING_PROMPT.md`, inspected existing `compound-v3` surface, manifests, and corpus, then verified current production Comet proxy markets against Compound III docs and `compound-finance/comet` deployments. Added `registryV2/surface/compound-v3/_deployments.json` with 28 cover contracts. |
| Claude Code or sub-agent research executed | done | Sub-agent `019e8389-e2fb-7652-b591-c69fdf6bc2a7` reviewed local Compound inventory and gap status. Claude Code CLI was attempted but unavailable because `claude -p ...` returned `Not logged in`. The Codex sub-agent supplied the independent second opinion for this run. |
| Claude/sub-agent exact prompt or command recorded | done | Sub-agent prompt asked for a no-edit review of Compound V3 local artifacts, contract inventory, manifest selector mapping, corpus strictness, and P2/P4 blockers. Claude command attempted: `claude -p --permission-mode auto --allowedTools Read,Grep,Glob -- <Compound P0 second-opinion prompt>`; result `Not logged in`. |
| Codex-only candidates listed | done | Codex candidate set was the 28 production Comet proxy markets in `registryV2/surface/compound-v3/_deployments.json`. Existing local candidates were 28 ABI snapshots and 28 coverage files under `registryV2/surface/compound-v3`. |
| Claude/sub-agent-only candidates listed | done | No additional verified Comet proxy candidates. Sub-agent agreed with the 28-cover local inventory and identified deferred work on full real-tx import plus semantic risk around Comet internal ledger transfer source modeling. |
| dropped-unverified candidates listed with reason | done | No unverified additional markets were accepted. Non-proxy support contracts from Compound deployment roots were scoped out because Compound docs state users interact with the Comet proxy address for each Compound III instance. |
| final contract inventory verified against first-party sources | done | `registryV2/surface/compound-v3/_deployments.json` records source `Compound III docs Networks table + compound-finance/comet deployments/*/*/roots.json`, fetched 2026-06-01, with 28 contracts and every row `decision=cover`. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | Not applicable. Compound V3 onboarding surface is a finite set of Comet proxy market deployments, not a pool/factory/vault child-address universe. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | Not applicable. Machine-readable finite deployment artifact is `_deployments.json` with `contracts.length=28`. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | Not applicable. Every Comet proxy in `_deployments.json` is dispositioned `cover` with reason `production Comet proxy; user-facing Compound III market entry point`. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | Concrete per-Comet manifests remain the chosen strategy. No protocol source resolver or factory materializer is needed for the 28 fixed Comet proxy markets. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | Explicitly not applicable because Compound V3 is not pool/factory/vault-heavy in this onboarding surface. |
| token-surface inventory completed or explicitly scoped out | done | Scoped to existing registry token coverage and Compound concrete protocol manifests. Comet ERC20-like selectors that overlap standard ERC20 are kept by concrete Compound manifests where present; no new token files were required for this inventory-only revalidation. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | Added `registryV2/surface/compound-v3/_deployments.json` with 28 production Comet proxies across Ethereum, Optimism, Unichain, Polygon, Ronin, Mantle, Base, Arbitrum, Linea, and Scroll. |
| `npm run check:surface` output recorded | done | `cd registryV2 && npm run check:surface` PASS. Compound line: `compound-v3: 28 deployed · 28 cover · 0 exclude (contract-inventory enforced vs Compound III docs Networks table + compound-finance/comet deployments/*/*/roots.json)`. Remaining WARNs are unrelated non-Compound ungated protocols. |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | Existing Comet COVER selectors map to existing domains: lending for `supply`, `supplyTo`, `supplyFrom`, `withdraw`, `withdrawTo`, `withdrawFrom`, `buyCollateral`; token for `transfer`, `transferFrom`, `transferAsset`, `transferAssetFrom`, `approve`, `approveThis`; permission for `allow`, `allowBySig`, and typed `Authorization` signing manifests. No new Tier3 action was required. |
| permission/fund-movement/red-flag selector review recorded | done | Permission selectors `allow`, `allowBySig`, and all 28 typed `Authorization` manifests are covered. Fund-movement selectors supply, withdraw, buyCollateral, transfer, transferFrom, transferAsset, and transferAssetFrom remain covered by concrete manifests. ERC20-like `approve` and Comet-specific `approveThis` are covered as token approval actions. |
| manifest files added/changed listed | done | No manifest files changed in this run. Existing Compound manifest set is 43 JSON files under `registryV2/manifests/compound-v3/comet`, covering 15 onchain selector templates plus 28 market-specific typed-data Authorization manifests. |
| enrichment/live_field decision recorded for every COVER action | done | Existing manifests retain decode-time fields and live input wiring. Lending supply/withdraw/buyCollateral corpus pins verify venue chain, comet, base asset, asset/collateral, amount, and recipient fields. Permission actions pin manager/authorizer/authorized/is_authorized. Token actions pin asset/token, amount, recipient, spender, and protocol fields. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | No new remote policy-RPC or enrichment endpoint was introduced. Existing live inputs in Compound manifests are covered through existing policy-sync/live input handlers and were exercised by `npm run check:manifest`, full validate, corpus, fuzz, and workspace tests. |
| Tier3 not needed or full Tier3 downstream contract completed | done | Tier3 not needed. Compound V3 reuses existing `lending`, `token`, and `permission` ActionBody variants and downstream lowering/effect/schema paths. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | Applicable existing downstream files include `crates/policy-server/asset-model/action`, `crates/policy-server/asset-model/transition/src/effect/lending/compound_v3`, `crates/policy-engine/src/lowering_v2/lending`, `crates/policy-engine/src/lowering_v2/token`, `crates/policy-engine/src/lowering_v2/permission`, and schema action registrations. No Tier3 file changed in this run. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `cd registryV2 && npm run check:manifest` PASS: representative build wrote `284 sourced callkey reps`; `v3-harness validate --representative-source-refs` reported `1416 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24, source-ref representative]`. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations 5000 --seed 42 --filter compound-v3 --json /tmp/compound-v3-fuzz-5000.json`. |
| iterations >= 5000 or justified lower bound | done | 5000 iterations used. Result: `total=2240000 pass=2240000 soft=0 fail=0 panicked=0 skipped=0`; `compound-v3 total=2240000 pass=2240000 soft=0 fail=0 panic=0`; domain histogram lending `1540000`, token `700000`. |
| fixed edge-case matrix recorded | done | Corpus and fuzz cover all existing Compound selector templates plus typed Authorization: supply, supplyTo, supplyFrom, withdraw, withdrawTo, withdrawFrom, buyCollateral, transfer, transferFrom, transferAsset, transferAssetFrom, approve, approveThis, allow, allowBySig, and market-specific Authorization typed data. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | Permission edges covered by `allow`, `allowBySig`, and typed Authorization grant/revoke cases. Value and amount boundaries covered by synthetic fuzz and strict corpus pins. Nested, array, opcode, and router path edges are not applicable to Compound Comet single-call manifests. Deadlines/nonces are represented in signed authorization corpus and manifest typed-data fields. |
| representative pass/error corpus entries committed or justified | done | `crates/integration-tests/data/golden/v3-decode/compound-v3/corpus.json` now has 70 pass entries and 70 semantic `expect_body` pin sets. No pass/error expectation flips were made. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | blocked | Checked `ETHERSCAN_API_KEY` and `crates/integration-tests/.env`; neither exists in this worktree. No Etherscan MCP tool was available in the exposed toolset. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | blocked | Cannot execute adapter-blind Etherscan txlist pull over the 28 P0 cover addresses without an Etherscan key or MCP adapter. |
| external tx pull target address count is nonzero and recorded | done | Dune calibration targeted 3 representative Comet addresses and returned rows. Planned Etherscan full target set is nonzero at 28 P0 cover addresses but blocked by unavailable Etherscan credentials. |
| Etherscan `api_calls_used` recorded | blocked | Etherscan `api_calls_used=0` because no credential or Etherscan MCP adapter was available. |
| Etherscan `raw_txs_seen` recorded | blocked | Etherscan `raw_txs_seen=0` because txlist pull was blocked by missing key/adapter. Dune calibration separately returned 120 rows. |
| Etherscan `unique_selectors_seen` recorded | blocked | Etherscan selector count unavailable. Dune calibration observed four targeted selectors: `0x095ea7b3`, `0x110496e5`, `0xf2b9fdb8`, and `0xf3fef3a3`. |
| Etherscan real tx coverage per COVER selector recorded | blocked | Full per-COVER real-tx coverage is blocked until the 28-address Etherscan pull is available. Dune calibration was partial and observed 4 of 5 targeted selectors, not all 15 onchain COVER selectors. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | Explicitly not applicable. Compound V3 P0 cover universe is the 28 fixed Comet proxies, not an expanding factory child universe. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | blocked | Unknown to-address bucketing requires the blocked adapter-blind Etherscan sweep. Dune calibration was address-scoped to selected known Comet proxies and did not provide a full unknown-address lane. |
| Dune MCP/API availability checked | done | Dune MCP `getUsage` succeeded before and after query. Account usage moved from `creditsUsed=35.562` to `creditsUsed=36.68`. |
| Dune usage baseline recorded | done | `crates/integration-tests/onboarding/compound-v3/dune-calibration-summary.json` records before and after credit usage, plan-free engine query, and usage delta. |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | Dune query `7630074`, execution `01KT1RJ1SCZ9RFWWTQV65GSA93`, used `block_date >= current_date - interval '30' day` over `ethereum.transactions`, `base.transactions`, and `optimism.transactions`. |
| Dune `executionCostCredits` / usage delta recorded | done | Dune `executionCostCredits=1.118`; usage delta `36.68 - 35.562 = 1.118`. |
| Dune rows returned / selected tx hashes recorded | done | Dune returned 120 rows. Summary artifact records selected sample hashes including Ethereum supply `0x9933ed58...`, Ethereum withdraw `0xae77cdc3...`, and Base withdraw `0xd6a5b687...`. |
| representative real-tx corpus/golden entries committed or justified | blocked | Existing local Compound corpus is now semantically pinned, but no new representative real-tx entries were imported from Dune because the Dune run was partial calibration and the Etherscan 28-market sweep was blocked. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --filter compound-v3 --require-expect-body` PASS: `70/70 matched`; `semantic expect_body: 70/70 pass entries pinned`. |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | Buckets: `COMPOUND-I0-INVENTORY` fixed by adding `_deployments.json`; `COMPOUND-CORPUS-STRICT` fixed by adding `expect_body` to all 70 pass corpus entries; `COMPOUND-REALTX-ETHERSCAN` blocked by missing Etherscan key/adapter; `COMPOUND-TRANSFER-SOURCE-MODEL` deferred for Comet internal ledger transfer source semantics. No synthetic hard failures, soft failures, or corpus mismatches remained after rerun. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | `COMPOUND-I0-INVENTORY` tied to 28 Comet proxy addresses and `npm run check:surface` warning removal. `COMPOUND-CORPUS-STRICT` tied to prior strict corpus failure with 70 missing `expect_body`. Synthetic rerun seed was `42`. Dune calibration tx hashes are recorded in `dune-calibration-summary.json`. |
| manifest/decoder/Tier3/harness change list recorded | done | Changed files: `registryV2/surface/compound-v3/_deployments.json`, `crates/integration-tests/data/golden/v3-decode/compound-v3/corpus.json`, `crates/integration-tests/onboarding/compound-v3/dune-calibration-summary.json`, and this evidence file. No manifest, decoder, Tier3, harness source, runtime, WASM, or schema file changed. |
| P2 rerun after fixes recorded | done | Reruns after fixes: `fuzz --iterations 5000 --seed 42 --filter compound-v3` PASS with `2240000` pass and zero fail/soft/panic; strict corpus PASS with `70/70 matched` and `70/70 pass entries pinned`; targeted test `cargo test -p policy-engine-integration-tests --test v3_decode_harness compound_v3 -- --nocapture` PASS with 11 tests passed. |
| corpus `expect` flips or exclusions justified | done | No `expect` values were flipped and no corpus entries were excluded. Only semantic `expect_body` assertions were added to existing pass entries. |
| remaining gaps have explicit defer/blocker disposition | done | Remaining dispositions: Etherscan real-tx full sweep blocked by missing key/adapter; representative real-tx import deferred until full sweep is available; Comet internal ledger transfer source modeling is deferred for a future semantic policy decision; Claude Code CLI remains unauthenticated but Codex sub-agent covered independent review. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | `cd registryV2 && npm run build` PASS: `manifests 755`, `tokens 3806`, skipped `239 sourced duplicate callkey(s)`, wrote `52847 callkey(s) + 82 typed-data entry(ies)` across 755 manifests. |
| registryV2 build-index vitest output recorded | done | Initial run failed in sandbox with `tsx` IPC `listen EPERM`. After `browser-extension` Yarn install, escalated rerun PASS: `node .yarn/releases/yarn-4.14.1.cjs vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts` => 1 file passed, 12 tests passed. |
| `npm run check:manifest` output recorded | done | `cd registryV2 && npm run check:manifest` PASS: representative source-ref build wrote 284 sourced callkey reps and validate reported `1416 single_emit manifest(s) OK, 0 structural errors`. Full gate also PASS: `npm run check:manifest:full` wrote `52847 callkey(s) + 82 typed-data entry(ies)` and `validate (all): 52657 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24]`. |
| `npm run check:surface` output recorded | done | `cd registryV2 && npm run check:surface` PASS. Compound I0 line: `28 deployed · 28 cover · 0 exclude`; remaining WARNs are unrelated Aave, Morpho, Hyperliquid, LayerZero, and Standard ungated entries. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | Explicitly not applicable because Compound V3 uses fixed Comet proxy deployments, not pool/factory/vault child universe materialization. |
| v3-harness coverage/fuzz/corpus outputs recorded | done | `v3-harness coverage` PASS: `callkeys=1550 typed_data_keys=82 unique_bundles=750 install_failures=0`; strategy counts included `single_emit 1496`. Compound fuzz PASS: `2240000 pass, 0 soft, 0 fail, 0 panic`. Corpus PASS before strict pinning was `70/70 matched`; after pinning strict corpus PASS is recorded below. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --filter compound-v3 --require-expect-body` PASS: `70/70 matched`; `semantic expect_body: 70/70 pass entries pinned`. |
| `cargo test --workspace` output recorded | done | `cargo test --workspace` PASS. Notable totals: `v3_decode_harness` 60 passed in 1192.65s; `policy-engine-wasm` 36 passed and 1 ignored; `declarative_v3_route` 78 passed; `policy_transition` 416 passed; doc tests passed or were ignored as documented. Database integration tests were ignored because `TEST_DATABASE_URL` is required. |
| wasm build output recorded if runtime/wasm/schema changed | done | Runtime/WASM/schema files did not change. Still ran `./scripts/wasm-build.sh`: sandbox run compiled release but failed installing `wasm-bindgen` due temp dir permission; escalated rerun PASS, wasm-opt ran, package ready at `crates/policy-engine-wasm/pkg`, artifacts copied to `browser-extension/backend/wasm` and `browser-extension/public/wasm`. No tracked generated diff resulted. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | `cargo fmt --all -- --check` PASS. `cd registryV2 && npm run typecheck` PASS. No Rust or TypeScript source file was changed, so clippy was not required beyond workspace tests and manifest harness gates. |
| exact staged files and commit hash recorded | done | Intended explicit staged files: `registryV2/surface/compound-v3/_deployments.json`, `crates/integration-tests/data/golden/v3-decode/compound-v3/corpus.json`, `crates/integration-tests/onboarding/compound-v3/dune-calibration-summary.json`, and `crates/integration-tests/onboarding/compound-v3/evidence.md`. Final commit hash is recorded in the session final response, following the existing Curve evidence pattern because a commit cannot contain its own final hash. |
| remaining WARNs/deferred selectors/actions listed with reason | done | WARNs: unrelated registry surface warnings for non-Compound protocols without deployment inventories or ungated contracts. Deferred: Etherscan full real-tx sweep and representative real-tx corpus import; semantic follow-up for Comet internal ledger transfer source fields. No Compound onchain COVER selector is intentionally excluded in this run. |
| no base/worktree merge performed unless user explicitly requested it | done | No base/worktree merge was performed. Work remains on branch `feat/Compound-onboarding`; user did not request merge or push. |

## Blockers

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| Etherscan 28-market adapter-blind real-tx sweep unavailable | `ETHERSCAN_API_KEY` absent and `crates/integration-tests/.env` absent; no Etherscan MCP adapter exposed | Provide Etherscan credentials or MCP adapter, then run txlist over all 28 `_deployments.json` cover addresses and record api calls, raw tx count, selectors, unknown addresses, and per-COVER coverage. |
| representative real-tx import not committed from Dune calibration | Dune query `7630074` was partial calibration over 3 representative addresses and 5 targeted selectors | After full Etherscan or broader Dune sweep, import representative real txs into corpus with semantic `expect_body` pins. |
| Comet internal ledger transfer source modeling deferred | Sub-agent review and local manifest semantics | Decide whether `transferFrom` and `transferAssetFrom` should expose source account semantics differently from current token transfer body before changing ActionBody or manifests. |
| Claude Code CLI unavailable | `claude -p ...` returned `Not logged in` | Authenticate Claude Code before a future Claude-specific second-opinion requirement; Codex sub-agent was used for this run. |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is
`done` or has a concrete, user-visible `blocked` disposition and this command
passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- compound-v3 --phase all
```
