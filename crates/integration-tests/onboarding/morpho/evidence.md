# Morpho Onboarding Evidence

## Run Metadata

| field | value |
|---|---|
| protocol | morpho |
| branch | feat/Morpho-onboarding |
| worktree | /Users/woojin/Desktop/upside_academy/project/policy-engine/.claude/worktrees/Morpho-onboarding |
| date | 2026-06-02 |
| main agent | Codex |
| base commit | 2e3c3297 |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Codex current-session research executed | done | Read `crates/integration-tests/ONBOARDING_PROMPT.md`; inspected existing Morpho surface, manifests, corpus, and Rust field-level tests with `rg -n "morpho\|Morpho"` and `find registryV2/surface/morpho registryV2/manifests/morpho crates/integration-tests/data/golden/v3-decode/morpho -maxdepth 3 -type f`. |
| Claude Code or sub-agent research executed | blocked | Attempted `timeout 60 claude -p ... --permission-mode default --allowedTools Read,Grep,Glob --add-dir <worktree>` twice. First attempt failed because `--permission-mode auto` is invalid for this local CLI; second attempt emitted `Decompression error: ZlibError` and timed out with exit 124. |
| Claude/sub-agent exact prompt or command recorded | blocked | Command prompt asked Claude to review Morpho P0/P1 candidate-only against `registryV2/surface/morpho`, `registryV2/manifests/morpho`, `crates/integration-tests/data/golden/v3-decode/morpho/corpus.json`, and first-party Morpho docs `https://docs.morpho.org/get-started/resources/addresses/`; blocked by local Claude CLI failure above. |
| Codex-only candidates listed | done | Codex verified existing COVER set: Morpho Blue supply, withdraw, borrow, repay, supplyCollateral, withdrawCollateral, setAuthorization, and EIP-712 Authorization on Ethereum and Base; added scoped I0 `_deployments.json`; identified official-docs multi-chain expansion gap. |
| Claude/sub-agent-only candidates listed | blocked | No Claude-only candidates available because the headless Claude run did not complete. |
| dropped-unverified candidates listed with reason | done | Dropped broad official-docs Morpho Blue deployments outside Ethereum/Base from committed `_deployments.json`; reason: current registry surface/manifests/corpus only cover chain 1 and 8453 and adding other user-facing deployments as COVER would fail I0 without first-party chain IDs, snapshots, manifests, and corpus. |
| final contract inventory verified against first-party sources | blocked | First-party source `https://docs.morpho.org/get-started/resources/addresses/` was fetched on 2026-06-02 and lists 47 Morpho Blue network rows. This PR verifies and gates the current Ethereum/Base surface only in `registryV2/surface/morpho/_deployments.json`; full multi-chain Morpho Blue inventory remains blocked by scope expansion. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | Not pool/factory-heavy for user pre-sign surface: Morpho Blue has one singleton per chain, and markets are `MarketParams` structs hashed into market IDs rather than child pool contracts. Official docs row count for Morpho singleton networks: 47. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | Not applicable: no per-market child contract universe is required for Morpho Blue. Machine-readable scoped contract inventory committed as `registryV2/surface/morpho/_deployments.json` with 8 Ethereum/Base rows. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | Not applicable for market child contracts. Scoped I0 dispositions: 2 Morpho singleton COVER rows and 6 IRM/oracle/pre-liquidation infrastructure EXCLUDE rows with reasons. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | Not applicable: concrete manifests are used for Morpho singleton selectors; no resolver/generator is needed for per-market child contracts. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | Not applicable: Morpho Blue is not pool/factory/vault-heavy for contract address universe purposes. |
| token-surface inventory completed or explicitly scoped out | done | Scoped out for this PR: Morpho Blue markets use existing ERC-20 loan/collateral token references. No Morpho-specific LP/share/debt token contract is minted for these direct actions; live accounting shares are protocol state, not a registered token surface. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | Added `registryV2/surface/morpho/_deployments.json` with official-docs Ethereum/Base Morpho Blue, IRM, ChainlinkOracleV2 Factory, and PreLiquidation Factory dispositions. |
| `npm run check:surface` output recorded | done | Escalated rerun PASS: `cd registryV2; npm run check:surface` => `Morpho [8453]` and `Morpho [1]` each `17 surface`, `7 cover`, `10 exclude`, `7 on-chain manifests`, `1 signed-struct`; `[I0] morpho: 8 deployed`, `2 cover`, `6 exclude`; final line `PASS`. |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | COVER selectors map to existing lending ActionBody: supply `0xa99aad89`, withdraw `0x5c2bea49`, borrow `0x50d8cd4b`, repay `0x20b76e81`, supplyCollateral `0x238d6579`, withdrawCollateral `0x8720316d`, setAuthorization `0xeecea000`; signed `Authorization` maps to `set_authorization`. No Tier3 required. |
| permission/fund-movement/red-flag selector review recorded | done | Fund movement selectors are supply, withdraw, borrow, repay, supplyCollateral, withdrawCollateral. Permission selectors are setAuthorization and Authorization typed-data. Excluded red flags: setAuthorizationWithSig as relayer submission, liquidate and flashLoan as non-user pre-sign core flow, createMarket and governance/admin as infra/governance. |
| manifest files added/changed listed | done | Existing manifest files reused: `authorization-sign@1.0.0.json`, `borrow@1.0.0.json`, `repay@1.0.0.json`, `set-authorization@1.0.0.json`, `supply-collateral@1.0.0.json`, `supply@1.0.0.json`, `withdraw-collateral@1.0.0.json`, `withdraw@1.0.0.json`. No manifest content changed. |
| enrichment/live_field decision recorded for every COVER action | done | Existing Morpho manifests provide `reserve_state`, position-derived user state, APY/share-price/liquidity/debt derived fields as appropriate. Authorization actions require no live enrichment beyond decoded chain/protocol/authorized/is_authorized. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | Local harness supplies deterministic live inputs for Morpho actions; strict corpus asserts reserve_state source metadata for lending fund-movement pass entries. No remote endpoint blocker for local decode/onboarding gates. |
| Tier3 not needed or full Tier3 downstream contract completed | done | Tier3 not needed: existing `lending` domain actions and `set_authorization` body cover all Morpho selectors in scope. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | Not applicable: no Tier3 files added or modified. Existing field-level Rust tests cover Morpho market ID hashing and authorization fields. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | Escalated rerun PASS: `cd registryV2; npm run check:manifest` => representative source-ref build wrote `1571 callkey(s) + 82 typed-data entry(ies)` and validate reported `1416 single_emit manifest(s) OK, 0 structural errors`. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations 5000 --seed 42 --filter morpho --json /tmp/morpho-fuzz-5000.json`. |
| iterations >= 5000 or justified lower bound | done | PASS with 5000 iterations per callkey; output `total=80000 pass=80000 soft=0 fail=0 panicked=0 skipped=0`; protocol row `morpho total=80000 pass=80000`. |
| fixed edge-case matrix recorded | done | Existing corpus covers 2 pass examples for each mainnet fund-movement/authorization selector, 2 expected-error excluded selectors createMarket/liquidate, 1 Ethereum Authorization typed-data fixture, 1 Base supply hand fixture, and 1 Base Authorization typed-data fixture. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | Permission: setAuthorization and Authorization typed-data pinned. Value/fund movement: asset, amount, recipient/on_behalf_of pinned. Nested/array/opcode/deadline/path are not applicable to Morpho Blue singleton ABI in this scope. |
| representative pass/error corpus entries committed or justified | done | Updated `crates/integration-tests/data/golden/v3-decode/morpho/corpus.json` so all 31 pass entries now have `expect_body`: existing Ethereum/Base fixtures plus 7 fresh Etherscan V2 Ethereum representatives and 7 Dune direct Base representatives from the 2026-06-02 adapter-blind/backfill sweeps. Two excluded-selector error entries remain expected error with no expect flip. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | `ETHERSCAN_API_KEY` was available from the workspace `crates/integration-tests/.env`; no key material was printed or committed. Etherscan V2 account `txlist` probe was executed through the API directly because no Etherscan MCP tool was exposed. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | Executed Etherscan V2 `account.txlist` over the 2 scoped Morpho singleton cover addresses, page 1, offset 10000, sort desc, without selector filtering. Summary artifact: `crates/integration-tests/onboarding/morpho/etherscan-cover-tx-summary.json`. |
| external tx pull target address count is nonzero and recorded | done | Etherscan target count was 2 scoped cover addresses: Ethereum and Base Morpho Blue singleton. Ethereum returned 10000 rows; Base returned an Etherscan V2 free-plan unsupported-chain response and was backfilled with Dune direct top-level singleton rows. |
| Etherscan `api_calls_used` recorded | done | `etherscan-cover-tx-summary.json` records `api_calls_used=2`. |
| Etherscan `raw_txs_seen` recorded | done | `etherscan-cover-tx-summary.json` records `raw_txs_seen=10000`. |
| Etherscan `unique_selectors_seen` recorded | done | `etherscan-cover-tx-summary.json` records `unique_selectors_seen=12`. |
| Etherscan real tx coverage per COVER selector recorded | done | `etherscan-cover-tx-summary.json` records 7/7 onchain COVER selectors observed on Ethereum: supply, withdraw, borrow, repay, supplyCollateral, withdrawCollateral, and setAuthorization. Base direct top-level 7/7 selector evidence and corpus import status are recorded in `dune-direct-base-summary.json`. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | Not applicable: no child pool/factory universe for Morpho market contracts. Pending external sweep target set is the two scoped Morpho singleton cover addresses. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | The fresh Etherscan sweep was scoped to the 2 committed P0 cover singleton addresses, so returned rows were already dispositioned as `cover`. Base direct Dune query also filtered `to = 0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb`. Broader 47-row official-docs multi-chain expansion remains a separately dispositioned scope blocker in the Blockers table. |
| Dune MCP/API availability checked | done | Dune MCP available. `getUsage` baseline returned billing period `2026-05-05` to `2026-06-05`, plan `community_fluid_engine_v2`, `creditsUsed=36.68`, `creditsQuota=2500`. |
| Dune usage baseline recorded | done | Baseline `creditsUsed=36.68`; after calibration `creditsUsed=36.91`; delta matches query result `executionCostCredits=0.23`. |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | Calibration query `https://dune.com/queries/7630558`, execution `01KT1WB5102VX8J76P5735YTHK`, used `call_block_date >= current_date - interval '7' day` over decoded Morpho Blue call tables. Base direct top-level backfill query `https://dune.com/queries/7633652`, execution `01KT32CB12GSR5QJV2HCYJZJ01`, used `block_time >= now() - interval '90' day` over `base.transactions`. |
| Dune `executionCostCredits` / usage delta recorded | done | Calibration `executionCostCredits=0.23`; Base direct top-level sweep `executionCostCredits=5.24`; Base repay `to_hex(data)` check `executionCostCredits=0.161`. |
| Dune rows returned / selected tx hashes recorded | done | Calibration returned 14 aggregate rows. Base direct top-level sweep returned 14 rows, up to 2 per selector, and `dune-direct-base-summary.json` records selected direct Base hashes including supply `0x7cd2dd9a...`, borrow `0x2e3ba028...`, repay `0x820bdf57...`, and setAuthorization `0x335029a4...`; calldata hex check query `7633691` returned 2 repay rows. |
| representative real-tx corpus/golden entries committed or justified | done | Existing Morpho corpus had 14 Ethereum real tx entries. This pass imported 7 additional fresh Etherscan V2 Ethereum representatives and 7 Dune direct Base representatives, one per onchain COVER selector on each chain, with semantic `expect_body` domain/action pins. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | PASS after fresh real-tx import: `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --filter morpho --require-expect-body` => `corpus: 33/33 matched`, `semantic expect_body: 31/31 pass entries pinned`. |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | No decode hard/soft/misdecode gaps after pinned corpus and fuzz. Fresh Etherscan closed Ethereum, and the Base free-plan Etherscan lane is closed through Dune direct top-level rows imported into corpus. Remaining non-real-tx gaps are blocked_multichain_surface for official-docs deployments beyond Ethereum/Base and blocked_claude_code for second-opinion execution. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | Fix `semantic-pin-morpho-corpus` tied to all 31 pass corpus entries; fix `i0-scoped-morpho-deployments` tied to current Ethereum/Base Morpho Blue surface; fresh real-tx import tied to the 7 selected Etherscan hashes in `etherscan-cover-tx-summary.json` and the 7 selected Base Dune hashes in `dune-direct-base-summary.json`; fuzz seed `42` verified no synthetic decode gaps. |
| manifest/decoder/Tier3/harness change list recorded | done | No manifest, decoder, Tier3, or harness source changes for the real-tx closure pass. Changed files are corpus fresh real-tx entries, scoped Morpho `_deployments.json`, `etherscan-cover-tx-summary.json`, `dune-direct-base-summary.json`, and this evidence file. |
| P2 rerun after fixes recorded | done | Reran strict corpus after fresh Ethereum Etherscan and Base Dune imports: 33/33 matched and 31/31 pass entries pinned. Reran fuzz after current full index: 80000/80000 pass. |
| corpus `expect` flips or exclusions justified | done | No `expect` values flipped. Existing createMarket/liquidate entries remain `expect:"error"` consistent with coverage exclusions. |
| remaining gaps have explicit defer/blocker disposition | done | Remaining blockers are listed in the Blockers table: official-docs multi-chain Morpho expansion beyond current Ethereum/Base scope and Claude Code local CLI failure. Base Etherscan free-plan txlist limitation is mitigated by Dune direct corpus import. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | Escalated rerun PASS: `cd registryV2; npm run build` => `done - 52847 callkey(s) + 82 typed-data entry(ies) written across 755 manifest(s)`. |
| registryV2 build-index vitest output recorded | done | Sandbox run failed with tsx IPC `listen EPERM`; escalated rerun PASS: `cd browser-extension; node .yarn/releases/yarn-4.14.1.cjs vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts` => `Test Files 1 passed`, `Tests 12 passed`. |
| `npm run check:manifest` output recorded | done | PASS: `cd registryV2; npm run check:manifest` => representative source-ref build `1571 callkey(s) + 82 typed-data entry(ies)` and `validate (all): 1416 single_emit manifest(s) OK, 0 structural errors`. |
| `npm run check:surface` output recorded | done | PASS: `cd registryV2; npm run check:surface` => Morpho surface rows clean and `[I0] morpho: 8 deployed`, `2 cover`, `6 exclude`; final line `PASS`. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | Not applicable: Morpho Blue is not pool/factory/vault-heavy for child contract universe purposes. |
| v3-harness coverage/fuzz/corpus outputs recorded | done | Coverage PASS summary `callkeys=1550`, `typed_data_keys=82`, `install_failures=0`; fuzz PASS `total=80000 pass=80000`; corpus strict PASS after fresh real-tx import `33/33 matched`, `31/31 pass entries pinned`. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | PASS command after fresh real-tx import: `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --filter morpho --require-expect-body`; output `corpus: 33/33 matched`, `semantic expect_body: 31/31 pass entries pinned`. |
| `cargo test --workspace` output recorded | done | PASS: `cargo test --workspace`; notable long test `v3_decode_harness` completed in `1259.61s`; workspace exited 0 with all non-ignored tests passing. |
| wasm build output recorded if runtime/wasm/schema changed | done | Not applicable for this PR's onboarding artifacts: no wasm/schema surface changed. `cargo test --workspace` included `policy_engine_wasm` unit and integration tests passing; follow-up Rust CI fixes were verified with full clippy and rustdoc gates. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | PASS: `cargo fmt --all -- --check`; PASS: `cargo clippy --all-targets --all-features -- -D warnings`; PASS: `env RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features`; PASS: `cd registryV2; npm run typecheck`. |
| exact staged files and commit hash recorded | done | Artifact commit `da5d1b2b` staged exactly: `registryV2/surface/morpho/_deployments.json`, `crates/integration-tests/data/golden/v3-decode/morpho/corpus.json`, `crates/integration-tests/onboarding/morpho/evidence.md`. Real-tx closure additionally stages `crates/integration-tests/onboarding/morpho/etherscan-cover-tx-summary.json`, `crates/integration-tests/onboarding/morpho/dune-direct-base-summary.json`, updated corpus, and this evidence file. Final commit hash is recorded in the session final response. |
| remaining WARNs/deferred selectors/actions listed with reason | done | Remaining WARNs are unrelated global surface warnings for aave and compound-v3 missing `_deployments.json`, plus 21 ungated protocol contracts across aave, hyperliquid, layerzero, standard. Morpho-specific deferred scope: official-docs Morpho Blue deployments beyond Ethereum/Base and local Claude CLI second-opinion failure. |
| no base/worktree merge performed unless user explicitly requested it | done | No merge into `main` or base worktree was performed. Work remains on branch `feat/Morpho-onboarding` in the dedicated worktree. |

## Blockers

| blocker | source | next action |
|---|---|---|
| Official Morpho docs list 47 Morpho Blue singleton network rows, but current registry surface/manifests cover only Ethereum and Base. | First-party docs `https://docs.morpho.org/get-started/resources/addresses/`, fetched 2026-06-02. | Decide whether Morpho onboarding scope should expand to all supported chains; if yes, add chain IDs, snapshots, coverage, manifests, corpus, and external sweeps per chain. |
| Claude Code second-opinion run unavailable. | Local `claude -p` attempts failed with invalid local mode, then `Decompression error: ZlibError` and timeout 124. | Retry after fixing local Claude CLI/session state, or use another sub-agent path for candidate-only review. |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is `done` or has a concrete, user-visible `blocked` disposition and this command passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- morpho --phase all
```
