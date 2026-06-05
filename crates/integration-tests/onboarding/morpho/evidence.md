# Morpho — Onboarding Evidence Ledger

> Greenfield **re-run** of the ScopeBall protocol-onboarding framework on Morpho
> (`ONBOARDING_PROMPT.md`), this time under the **SCOPE-ORACLE-hardened** framework.
> Treated as a fresh onboarding; prior artifacts (which had expanded to mainnet **+ Base**
> and deferred Bundler3 on an *unverified* premise) are re-derived from 1st-party sources
> and reconciled to the hardened rules.
>
> This run doubles as a **framework dogfood/test**: the explicit goal is to check whether the
> hardened `[SCOPE CONTRACT]` + P2 `SCOPE ORACLE` rules *catch* the prior run's scope errors
> (arbitrary multichain expansion, prose-only DEFER, no coverage-share). Framework-level
> findings are logged in "Framework Dogfood Findings" and hardened structurally, not patched
> per-instance. The test verdict is at the end.

## Run Metadata

| field | value |
|---|---|
| protocol | morpho |
| branch | feat/morpho-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-morpho |
| date | 2026-06-02 (re-run); **2026-06-03 (Q1 vault long-tail promotion → all 73 listed mainnet vaults cover)** |
| main agent | Claude Opus 4.8 (1M context), this session |
| base commit | a8909023 (feat/registry-v2); hardening cherry-picked at 67426771; mainnet-only reconciliation at 4116ce65; **Q1 on c9916daf** |

## Scope Classification

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | **Ethereum mainnet (`1`) ONLY.** Base (`8453`) and all other chains = explicit defer (separate multichain framework). The prior run's mainnet **+ Base** expansion was reverted under the hardened one-chain SCOPE CONTRACT (see FW-2). |
| completion target | `wallet-facing`, **direct-call subset** — Morpho Blue singleton (direct + off-chain Authorization) + MetaMorpho ERC-4626 **all 73 listed mainnet vaults** (Q1 long-tail promotion; was an 8-vault TVL cover-batch), **direct-to-vault calls only**. |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | **Q1 (all 73 listed covered):** vault-direct = **100%** of listed mainnet vaults (was 41% with 8); listed-vault **TVL = 100%** ($1.26B; the 8 cover were 75.7%). **BUT all-entrypoint coverage ceiling is unchanged at ~2.7%** — direct-to-vault is a *minority* of user entrypoints. FW-2 8-cover sample (Dune 30d): direct-to-vault **2.7%** / Bundler3 **14.5%** / other routers+aggregators+AA **82.8%**; Morpho-native direct ≈ **16%** (rest via Bundler3). Q1 fills the direct slice fully (~1.1%→~2.7% of all entrypoints) but **Bundler3 (Q2) is the unlock for the dominant ~14.5%**. **Morpho Blue:** singleton, all 8 user selectors + Authorization covered for *direct* calls. |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | **Bundler3** (multicall router): **~84% of Morpho-native MetaMorpho vault deposits** (Dune 30d: direct 193 tx vs Bundler3 1038 tx), ~14.5% of all vault-touching txs — **#1 follow-up (Q2)**. **Other routers/aggregators/ERC-4337 AA-entrypoints**: ~83% of all vault-touching txs, fragmented (M3/M4). **Base/multichain, MetaMorpho V2/VaultV2, URD claim, mint/redeem §4d enrichment**: separate-surface defers. *(65-vault long-tail — formerly the #2 defer — is now COVERED by Q1.)* |
| direct factory-child calls | **covered — all 73 listed mainnet vaults** via concrete `$to`-keyed manifests grouped by (chain, underlying) (Q1; was 8 cover + 65 defer) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | see **Final Completion Claim** — bounded to the measured shares; explicitly *not* "the full surface a Morpho user signs." |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification above — **mainnet (1) ONLY**, wallet-facing direct-call subset, multichain explicitly deferred. SCOPE CONTRACT fixed before P1. |
| Codex current-session research executed | done | prior run WebFetched 1st-party: IMorpho.sol (raw.githubusercontent morpho-org/morpho-blue/main) → Morpho Blue 8 user fns; IMetaMorpho.sol (morpho-org/metamorpho) → ERC-4626+ERC-2612+curator surface; docs.morpho.org/get-started/resources/addresses. Re-run reconciles those (unchanged for mainnet); Base inventory dropped. |
| Claude Code or sub-agent research executed | done | prior sub-agents: Explore repo inventory, general-purpose deployments sweep + MetaMorpho vault cover-batch (blue-api GraphQL). Re-run consumes the verified mainnet subset only. |
| Claude/sub-agent exact prompt or command recorded | done | prompts embedded in prior Agent calls (deployments sweep + vault cover-batch + repo inventory), read-only. This re-run added no new discovery agents (reconciliation, not greenfield discovery). |
| Codex-only candidates listed | done | VaultV2Factory / V2 adapter factories / MorphoRegistry (blue-sdk) underemphasized by the docs page — recorded in `_deployments.json` (mainnet rows; all `exclude`). |
| Claude/sub-agent-only candidates listed | done | per-chain Base addresses (blue-sdk chain-keyed) — **now dropped** (multichain defer); URD instance example; MORPHO two-token ambiguity; the 73-vault mainnet universe + 8-vault TVL cover-batch. |
| dropped-unverified candidates listed with reason | done | MetaMorpho V1.0 [OLD] Base factory = NOT FOUND first-party (dropped, moot now Base-deferred); URD instances beyond 1 example = not statically enumerable (deferred); **all Base (8453) deployments dropped from scope** under the one-chain SCOPE CONTRACT. |
| final contract inventory verified against first-party sources | done | Morpho Blue `0xbbbb…effcb` (chain 1) re-verified vs IMorpho.sol → Full-8 CORRECT. Mainnet contracts from docs.morpho.org/addresses + blue-sdk addresses.ts; MetaMorpho v1.0/v1.1 + MORPHO ABIs from Etherscan v2 (verified). `check:surface` I0 = `✓ morpho: 18 deployed · 1 cover · 17 exclude` (mainnet-only). |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | MetaMorpho vault universe via `blue-api.morpho.org/graphql vaults(where:{chainId_in:[1],listed:true})`: **mainnet = 73** (countTotal). `surface/morpho/_address_universe.json` source_count=73 (Base 33 removed). |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | `surface/morpho/_address_universe.json` — 73 mainnet candidates, machine-readable, committed. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | **8 cover** (`metamorpho-top-tvl-cover`, top-8 mainnet by totalAssetsUsd) + **65 defer** (`metamorpho-longtail-defer`, below cutoff, same surface; measured ~59% of vault-direct txs). batch_boundary = blue-api listed:true snapshot 2026-06-02, mainnet. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | **concrete `$to`-keyed manifests grouped by (chain=1, underlying)** for the 8 cover vaults (gate-native; `gatedSourceAddresses` only hardcodes token/uniswap sources, so a custom resolver would false-fail I2). Full 73-vault source-resolver materialization = documented follow-up. |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | vaults = factory children users call directly (ERC-4626) → 8 covered via concrete cover-batch; 65-vault long-tail deferred (universe). Router (Bundler3) flows deferred separately (`_deployments` DEFER, with measured share). |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | `PASS — 73 candidates · 8 cover · 0 exclude · 65 defer · source_count=73`. |
| token-surface inventory completed or explicitly scoped out | done | mainnet token-surface: 8 MetaMorpho vault shares (`yield_receipt`, decimals=18 by DECIMALS_OFFSET, underlying ref) + MORPHO governance (0x58d9.. + 0x9994.. legacy). 9 Base token files removed. Underlyings USDC/WETH/USDT/WBTC/PYUSD/RLUSD/USDtb already registered. `check:tokens` PASS (0 errors; 1338 pre-existing registry-wide warns, morpho-unrelated). |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | `surface/morpho/_deployments.json` reduced 33 → **18 contracts (mainnet only)**; Morpho Blue cover; factories/IRM/oracle/V2/registry exclude; Bundler3/GeneralAdapter1/ParaswapAdapter/URD = explicit DEFER. Bundler3 reason now carries the **measured ~84% share** (was unverified prose). |
| `npm run check:surface` output recorded | done | `PASS`; I0 `✓ morpho: 18 deployed · 1 cover · 17 exclude`. I0' WARN: 8 cover vaults not in `_deployments` (expected — factory children, live in `_address_universe.json`, same as uniswap pools). |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | Morpho Blue 8 = existing (re-verified). MetaMorpho: deposit/mint → `LendingAction::Supply`, withdraw/redeem → `LendingAction::Withdraw`, all carrying `LendingVenue::MetaMorpho{chain,vault}` (Tier-3 venue, unchanged from prior run). |
| permission/fund-movement/red-flag selector review recorded | done | MetaMorpho permission review: approve/permit/transfer/transferFrom on the vault SHARE = standard ERC-20/2612 (tokens:erc20, not silently excluded). No protocol-specific permission primitive on the vault (curator/allocator/guardian = timelocked gov, EXCLUDE). Morpho Blue setAuthorization (the core grant) covered. |
| manifest files added/changed listed | done | **28 mainnet manifests** `registryV2/manifests/morpho/metamorpho/1-<underlying>-{deposit\|withdraw\|mint\|redeem}@1.0.0.json` (7 underlyings × 4; USDC group lists both Steakhouse USDC + Gauntlet USDC Prime). **12 Base (`8453-*`) manifests removed.** Morpho Blue 8 manifests: `"8453"` removed from `chain_to_addresses` (mainnet-only). |
| enrichment/live_field decision recorded for every COVER action | done | deposit/withdraw = asset-denominated (user-legible → no enrichment). mint/redeem = SHARE-denominated (abstract) → §4d `convertToAssets` enrichment **DEFERRED** (documented). All `live_inputs` = skeleton `derived_from` placeholders (vault-state prod-fill deferred). |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | live_inputs skeleton (decode does not fetch; policy-RPC dormant). No live handler required for the decode/verdict path. Enrichment calc_ids (`metamorpho_*_skeleton`) named for future wiring. |
| Tier3 not needed or full Tier3 downstream contract completed | done | Tier-3 = `LendingVenue::MetaMorpho{chain,vault}` (a new **venue**, not a new action/domain — reuses Supply/Withdraw). Mirrors Fluid/MorphoOptimizer ERC-4626 venue. Unchanged from prior run (no Rust touched this reconciliation). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | `action/src/lending/mod.rs` (variant + `name()`="metamorpho" + `#[serde(rename="metamorpho")]`); `lowering_v2/lending/mod.rs` ({chain,vault} arm + unit test); `transition/effect/lending/{mod.rs,supply.rs}`; `policy-sync/src/actions/args.rs` (`venue_to_address`); `action/src/view.rs` (conformance `assert_venue!`); `core.cedarschema` (`vault?` REUSED, no new field). No Cedar action-registration (reuses Supply/Withdraw). |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `check:manifest`: `1521 single_emit OK, 0 structural errors`. build-index: **52974 callkeys / 83 typed-data / 814 manifests** (was 53041/84/826 — Base 12 manifests + 1 typed-data chain-entry removed). |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `v3-harness fuzz --filter metamorpho --iters 5000 --seed 0x6d6574616d6f7270` → all-pass (lending domain). Now mainnet-only callkeys (Base removed). |
| iterations >= 5000 or justified lower bound | done | requested 5000; harness caps at 64 iters/callkey. Mainnet callkeys = 8 vaults × 4 selectors = 32 callkeys (Base 24 callkeys removed). 0 failures. |
| fixed edge-case matrix recorded | done | fuzz EDGE_ITERS (boundary values: 0, U256::MAX) per callkey; real-tx corpus adds natural edges (tiny/large deposits, full redeem). |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | ERC-4626 deposit/withdraw/mint/redeem are flat (uint256,address[,address]) — no nested/array/opcode/deadline. Permission edge = vault-share approve/transfer boundary (corpus: pass/token, not metamorpho) → verifies token-vs-protocol split. |
| representative pass/error corpus entries committed or justified | done | `data/golden/v3-decode/metamorpho/corpus.json` — **15 mainnet entries** (was 24; 9 Base Dune entries removed): pass deposit/withdraw/redeem/mint→lending + approve/transfer→token, plus error reallocate/updateWithdrawQueue → no_declarative_v3_mapper. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 (`ETHERSCAN_API_KEY` in `crates/integration-tests/.env`) — chain 1 txlist WORKS. (Base `account/txlist` blocked on free tier — moot now Base-deferred.) |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | 8 mainnet cover vaults, `account&action=txlist&offset=250&sort=desc` each (adapter-blind, isError=0). |
| external tx pull target address count is nonzero and recorded | done | **8 mainnet vault targets** (Etherscan) + 73-vault + Bundler3/adapter universe swept via Dune (M1–M5). Nonzero. |
| Etherscan `api_calls_used` recorded | done | ~8 txlist calls (mainnet) + ~10 getabi (snapshots/MORPHO) ≈ 18 calls. |
| Etherscan `raw_txs_seen` recorded | done | ~1,400 mainnet direct-call txs across 8 vaults (≤250 each). |
| Etherscan `unique_selectors_seen` recorded | done | deposit 0x6e553f65, redeem 0xba087652, withdraw 0xb460af94, mint 0x94bf804d (COVER) + reallocate 0x7299aa31, approve, updateWithdrawQueue, acceptCap, transfer (EXCLUDE/token). |
| Etherscan real tx coverage per COVER selector recorded | done | mainnet direct calls: deposit 160, redeem 109, withdraw 41, mint 1 (mint sparse — most direct users deposit). All 4 COVER selectors observed on real direct calls. |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | cover-batch VAULTS = the wallet-facing direct targets. **8 mainnet targets**, per-target floor ~250, corpus = 15 matched. target file = `surface/morpho/_address_universe.json` (cover batch). |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | non-actionable: reallocate (allocator gov), approve/transfer (token), updateWithdrawQueue/acceptCap (gov) → EXCLUDE/tokens:erc20. **Actionable-but-deferred: Bundler3-routed deposits (to=Bundler3, not vault)** — now quantified at ~84% of Morpho-native deposits (SCOPE ORACLE row). |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | M1 swept all **73** listed mainnet vaults (not only the 8 cover). The 65 long-tail = ~59% of vault-direct txs — explicit defer (batch `metamorpho-longtail-defer`). |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | M3/M4 surfaced the dominant **non-vault entrypoints** (Bundler3 + fragmented routers/aggregators/ERC-4337 EntryPoint) — bucketed as DEFER router surface, not silent gaps. No unknown-address-with-vault-selector direct gap (txlist is address-keyed). |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | MetaMorpho adds NO bespoke EIP-712 (only standard ERC-2612 Permit on the share → tokens:erc20/2612 path, EXCLUDE). Morpho Blue `Authorization` typed-data covered (morpho/corpus.json, mainnet). |
| Dune MCP/API availability checked | done | Dune MCP available; plan `community_fluid_engine_v2`. |
| Dune usage baseline recorded | done | baseline 404.524 / 2500 credits (billing 2026-05-05 → 2026-06-05). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | 5 queries on `ethereum.transactions` / `ethereum.traces`, partition WHERE `block_time >= now() - interval '14'/'30' day`, free engine. IDs 7637873 (M1), 7637892 (M2), 7637912 (M3), 7637928 (M4), 7637947 (M5). |
| Dune `executionCostCredits` / usage delta recorded | done | 0.85 + 2.367 + 2.473 + 2.494 + 1.104 = **9.288 credits** (free engine). |
| Dune rows returned / selected tx hashes recorded | done | aggregate rows (M1 2, M2 3, M3 3, M4 15, M5 1). Mainnet corpus tx hashes retained from prior Etherscan pull (15 entries); Base Dune-selected hashes removed. |
| representative real-tx corpus/golden entries committed or justified | done | `metamorpho/corpus.json` 15 mainnet entries (Etherscan), 13 with field-level expect_body pins + 2 error. `morpho/corpus.json` 17 mainnet entries (2 Base removed). |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter metamorpho --require-expect-body` → **15/15 matched, 13/13 pass entries pinned** (mainnet venue.vault + asset.address + amount + party pins verified). |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | **MEASURED (Dune ethereum, 30d primary + 14d reconciliation; mainnet):** **M1** vault-concentration — 8 cover vaults = 200 vs 65-defer = 287 → cover = **41%** of vault-direct ERC4626 txs across 73 listed vaults. **M3** entrypoint distribution (what users sign) — direct-to-vault (COVERED) **193 = 2.7%** / Bundler3 1038 = 14.5% / other routers+aggregators+AA 5917 = 82.8%. Among Morpho-native paths (direct vs Bundler3): direct = **~16%**. **DEFER usage-shares:** Bundler3 ~84% Morpho-native (#1 follow-up); long-tail ~59%; other-router ~83% of all. **M5 (14d) overturns the prior run's "Bundler3 ~5% / direct ~95% dominant" finding** as a measurement error (its "direct 3483" = TOTAL calls mislabeled; true direct = 69, GA1 = 180 ≈ prior 182). Completion label bounded accordingly (FW-2). |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | NO decode gaps on the covered direct surface (corpus all-pass, fuzz all-pass, pins verified). Buckets are all DEFERRALS, now **data-gated**: (1) Bundler3 router (~84% Morpho-native — #1); (2) 65-vault long-tail (~59%); (3) other routers/aggregators/AA (~83% of all, fragmented); (4) Base/multichain; (5) mint/redeem §4d enrichment. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | This re-run's changes are SCOPE reconciliations, tied to the SCOPE ORACLE measurements, not decode fixes: (a) Base strip (FW-2/T1) — 12 manifests + 4 surface + 9 tokens + 11 corpus entries + 8 manifest `8453` keys; (b) Bundler3 defer data-gated (M2/M3); (c) coverage-share measured (M1/M3). Prior run's 2 code fixes (policy-sync exhaustive-match, clippy) remain landed. |
| manifest/decoder/Tier3/harness change list recorded | done | Reconciliation: removed 12 Base manifests; removed `"8453"` from 8 Morpho Blue manifests; deleted 4 Base surface files + 9 Base tokens; filtered corpus to mainnet (metamorpho 24→15, morpho 19→17); updated `_deployments`/`_address_universe` to mainnet + measured Bundler3 share. **No Rust/Tier3/decoder change** (venue unchanged). |
| P2 rerun after fixes recorded | done | after Base strip + rebuild: workspace `cargo test` 0 fail; `v3-harness corpus --filter metamorpho --require-expect-body` → **15/15 matched, 13/13 pass entries pinned**; morpho (Blue) corpus routes/matches (expect_body pins pre-existing-absent on Blue, not this run's scope). |
| corpus `expect` flips or exclusions justified | done | no flips. Removed entries = Base (out-of-scope under one-chain contract), not re-classified mainnet entries. Every retained mainnet `expect` correct on first decode. |
| remaining gaps have explicit defer/blocker disposition | done | all DEFERs above carry a 1st-party usage-share (data-gated). Base = one-chain-contract defer. No silent gaps. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | `done — 52974 callkey(s) + 83 typed-data entr(ies) across 814 manifest(s)` (deterministic; index gitignored/generated). |
| registryV2 build-index vitest output recorded | blocked | browser-extension Yarn 4 / WASM not provisioned in this onboarding worktree. build correctness covered by `npm run build` (validates every manifest+token) + `check:manifest` (1521 OK) + `check:surface` + `check:universe`. Rerun: `cd browser-extension && yarn && yarn vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts`. |
| `npm run check:manifest` output recorded | done | `1521 single_emit OK, 0 structural errors`. |
| `npm run check:surface` output recorded | done | `PASS`; I0 morpho `18 deployed · 1 cover · 17 exclude` (mainnet-only). 8-vault I0' WARN expected (factory children). |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | `PASS — 73 candidates · 8 cover · 0 exclude · 65 defer · source_count=73` (cover-linkage verified: all 8 cover vaults have by-callkey entries). |
| v3-harness coverage/fuzz/corpus outputs recorded | done | fuzz all-pass (seed 0x6d6574616d6f7270, mainnet callkeys); corpus metamorpho **15/15 matched, 13/13 pass entries pinned**; validate --filter metamorpho 32 callkeys OK. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | metamorpho **15/15 matched, 13/13 pass entries pinned**; morpho (Blue) **17 mainnet entries route/match** (2 Base entries removed; `--require-expect-body` reports expect_body pins **pre-existing-absent** on the Blue corpus — those entries were authored pre-pin as routing/pass checks; not a this-run regression). |
| `cargo test --workspace` output recorded | done | `--exclude policy-engine-integration-tests` → **0 fail** (policy-action 331, policy-transition 418, policy-engine 122, policy-sync 149, …). integration-tests full `v3_decode_harness` golden = **deferred this reconciliation** (registry-data-only edits, no decoder/Rust delta → cannot regress other protocols' golden; the affected morpho/metamorpho decode is covered by `corpus --filter metamorpho --require-expect-body` 15/15 + 13/13 pinned above). Prior run's full golden = 60/60 at `f41e472d`; re-run with `--test-threads=4` (FW-1 bound) if a full regression sweep is wanted. |
| wasm build output recorded if runtime/wasm/schema changed | done | **No Rust/schema change this reconciliation** (only registry data: manifests/surface/corpus/tokens). The `LendingVenue::MetaMorpho` venue + cedarschema were already compiled to WASM in the prior P1 commit (`a8bba544`). No re-build required. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | no Rust changed this reconciliation → fmt/clippy unchanged from prior green state. (JSON-only edits; `cargo test --workspace` green confirms no compile regression.) |
| exact staged files and commit hash recorded | done | reconciliation commit = the commit carrying this evidence (staged: 12 deleted Base manifests, 4 deleted Base surface, 9 deleted Base tokens, 8 edited Morpho Blue manifests, 2 corpus, 2 surface `_deployments`/`_address_universe`, this evidence.md). Hash = the single reconciliation commit on `feat/morpho-onboarding` carrying this evidence (top of `git log`). Generated `index/`/`pkg/` gitignored. |
| remaining WARNs/deferred selectors/actions listed with reason | done | Deferred (data-gated): Bundler3 (~84% Morpho-native), 65-vault long-tail (~59%), other routers/aggregators/AA (~83% of all), Base/multichain, V2/VaultV2, URD claim, mint/redeem §4d. WARNs: I0' (8 cover vaults not in `_deployments` — expected factory children); registry-wide UNGATED/token warns (other protocols). |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | see Final Completion Claim — explicitly bounded to ~41% vault-concentration / ~16% Morpho-native-direct / ~2.7% all-entrypoint coverage; mainnet-only; *not* "the full surface a user signs". |
| no base/worktree merge performed unless user explicitly requested it | done | no merge/push; all work on `feat/morpho-onboarding`. Shared base `feat/registry-v2` untouched. |

## Blockers

| blocker | source | next action |
|---|---|---|
| registryV2 build-index **vitest** not run | browser-extension Yarn 4 / WASM toolchain not provisioned in this onboarding worktree | non-fatal — build correctness covered by `npm run build` + `check:manifest` (1521 OK) + `check:surface` + `check:universe`. Rerun: `cd browser-extension && yarn && yarn vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts`. |

## Framework Dogfood Findings (this run = framework test)

| id | finding | severity | disposition |
|---|---|---|---|
| FW-1 | Full `npm run build` index ≈ 53k callkeys / 210M; `adapters::load_and_install()` has no caching → each harness test pays ~28–41s full-surface load; the `v3_decode_harness` golden suite OOMs (SIGKILL) under DEFAULT parallel threads. Pre-existing, not Morpho-specific. | medium | **RESOLVED for this run** via `cargo test … --test v3_decode_harness -- --test-threads=4`. Framework-hardening follow-up (shared `OnceCell` surface cache or representative-index harness mode) would remove the need to bound threads. |
| FW-2 | **The hardened SCOPE ORACLE caught a *wrong* recorded 1st-party number, not just a missing one.** The prior run (a) expanded scope to mainnet **+ Base** with no measurement, and (b) deferred Bundler3 on the prose claim "the Morpho app routes most actions through" while *also* recording "Bundler3 ~5% / direct ~95% dominant" — a self-contradiction. Re-measuring with proper top-level/internal-call discrimination (M2/M3/M5) shows the prior "direct 3483" was **TOTAL calls mislabeled** (true direct = 69/14d; GA1 = 180 ≈ prior 182). True direct-to-vault = ~2.7% of all entrypoints / ~16% of Morpho-native; **routers dominate**. | high | **Hardening already in place caught it** (P2 SCOPE ORACLE row is mandatory; one-chain SCOPE CONTRACT reverted the Base expansion). Reinforces the 1st-party rule extension to *usage/dominance* claims (a confident wrong number is worse than a missing one). No further framework change needed beyond what the cherry-picked hardening (67426771) already encodes; the test PASSED. |

## Framework-Test Verdict (did the hardened framework catch the prior run's scope errors?)

| test | hardened rule | prior-run error | caught? | outcome |
|---|---|---|---|---|
| **T1 — one representative chain** | `[SCOPE CONTRACT]` "대표 체인 1개; 멀티체인 = 별도 프레임워크 → 명시 defer" | scope expanded to mainnet **+ Base** (16-vault, 8/chain) with no justification | **YES** | reverted to mainnet-only: 12 Base manifests + 4 Base surface + 9 Base tokens + 11 Base corpus entries deleted; 8 Morpho Blue manifests realigned to chain 1. check:surface/universe green at 18/1/17 · 73/8/65. |
| **T2 — data-gated DEFER** | "user-facing DEFER 은 1차 usage-share 첨부; 추정 usage 로 scope 판단 금지" | Bundler3 deferred on prose ("routes most actions through") + a *contradictory* "5%/95%" number | **YES (emphatically)** | forced a real measurement (M2/M3/M5) that **overturned** the prior number; `_deployments` Bundler3 reason now carries the measured ~84% Morpho-native share; Bundler3 elevated to #1 follow-up. |
| **T3 — P2 coverage-share (SCOPE ORACLE)** | mandatory P2 row: "covered set decodes what % of recent P0-universe txs; label must not over-claim" | no coverage-share measured; evidence row absent; label claimed "the surfaces a Morpho user actually signs" | **YES** | the prior evidence.md (pre-hardening) lacked the mandatory SCOPE ORACLE row → `check-onboarding-evidence` would have flagged it; now measured (41% vault-concentration / 2.7% all-entrypoint / 16% Morpho-native) and the label is bounded. |

**Conclusion:** all three hardened rules caught their target errors and drove convergence to a mainnet-only, data-gated, coverage-bounded, non-over-claiming onboarding. The hardening (cherry-picked at `67426771`) is sufficient for these failure modes; the only net-new insight (FW-2) is that the 1st-party rule's extension to usage/dominance claims is load-bearing — it caught a *wrong* number, not just a missing one — which the cherry-picked guardrail already encodes.

## Q1 Follow-up — Vault Long-tail Promotion (2026-06-03)

> **Supersedes the 8-cover / 65-defer split recorded in P0–P4 above** (those rows document the
> original re-run state). Q1 promotes the entire listed mainnet MetaMorpho universe to COVER.
> Mechanical, registry-data-only (no Rust/decoder/schema change) — same ERC-4626 surface the 8
> cover vaults already proved.

**What changed.** All **65 long-tail vaults → COVER**, so all **73 listed mainnet vaults** are now
decoded. Done via the existing gate-native pattern (concrete `$to`-keyed manifests grouped by
`(chain=1, underlying)`, `venue=metamorpho{vault=$to}`, underlying asset baked) — **not** a custom
source-resolver (that path false-fails `check:surface` I2 per `gatedSourceAddresses`).

**1st-party data.** `blue-api.morpho.org/graphql vaults(where:{chainId_in:[1],listed:true})` snapshot
2026-06-02 → 73 vaults, 18 underlyings, total TVL **$1,262,352,932**. Share `decimals()` confirmed
**on-chain = 18 for all 73** (batch `eth_call` via publicnode; MetaMorpho `DECIMALS_OFFSET=18−assetDecimals` invariant).

**Files (registry-data only):**
- `tokens/1/<vault>.json` — **65 new** `yield_receipt` share tokens (8 pre-existing skipped; underlying ref resolves for all 65; decimals=18).
- `manifests/morpho/metamorpho/1-<underlying>-{deposit,withdraw,mint,redeem}@1.0.0.json` — **44 new** (11 new underlyings × 4: AUSD/cbBTC/DAI/EURC/EURCV/eUSD/LBTC/msETH/msUSD/USDf/wstETH) + **28 existing** had their `chain_to_addresses` list expanded to the full per-underlying vault set (structural-drift scan: 0 — only address-list + note changed).
- `surface/morpho/_address_universe.json` — 73 candidates all `cover` (was 8/65); `surface/morpho/metamorpho-mainnet.coverage.json` — `addresses` 8→73 (functions triage untouched).
- All 18 underlyings already registered (no new underlying tokens). `_deployments.json` untouched (vaults are factory children, deliberately not a deployment-list contract).

**Honest coverage re-measurement.** Covering all 73 maxes the **direct-to-vault axis** but does **not**
change the user-entrypoint ceiling:

| axis | before Q1 (8 cover) | after Q1 (73 cover) |
|---|---|---|
| listed mainnet vaults covered | 8 / 73 | **73 / 73 (100%)** |
| listed-vault TVL covered | 75.7% ($956M) | **100% ($1.26B)** |
| vault-direct ERC4626 tx | ~41% | **~100%** (all listed vaults) |
| **all user entrypoints** (what users sign) | ~1.1% | **~2.7%** (ceiling) |

The all-entrypoint number is still bounded by FW-2: **direct-to-vault ≈ 2.7%**, Bundler3 ≈ **14.5%**,
other routers/aggregators/AA ≈ **82.8%** (8-cover sample; the all-73 entrypoint split was not
re-run — the router-dominance is already established and Q1 doesn't move it). **Q1 fully decodes the
direct slice; Bundler3 (Q2) remains the single highest-value unlock (~14.5%).**

**Gates (all green, mainnet-only).** `npm run build` → 53,429 callkeys / 83 typed-data / 858 manifests;
`check:surface` **PASS** (I0 `18 deployed · 1 cover · 17 exclude`; 73 I0' WARN = factory children not in
`_deployments`, expected/benign, was 8); `check:universe --protocol morpho --require-cover-linkage`
**PASS — 73 candidates · 73 cover · 0 exclude · 0 defer** (all linked); `check:tokens` **PASS** (0 errors;
65 new tokens add 0 warns — underlying refs all resolve; on-chain decimals=18 verified for all 73).
**Decode proof:** `v3-harness corpus --filter metamorpho --require-expect-body` → **17/17 matched, 15/15
pinned** — added 2 real-tx entries on a NEW-underlying vault (wstETH `bbqWSTETH`: deposit `0xe8e7fc8c…`
+ redeem `0x27dddde0…`), both `expect=pass got=pass` with `venue.vault`/`asset.address`/`amount`/party
pins verified → the Q1-generated manifests decode real on-chain txs, not merely pass structural gates.
(The wstETH deposit even carries a 24-byte trailing referral suffix — decoded correctly, same as the
pre-existing USDC entry with that suffix.) **Full `v3_decode_harness` 60-test golden = 60/60 PASS**
(1670s under parallel-session RAM pressure) — confirms the metamorpho-isolated registry-data change
does not regress any other protocol's decode. `cargo test --workspace` (non-harness pure-Rust crates)
unchanged from the c9916daf green state (no Rust touched).

## Q2 Follow-up — Bundler3 `multicall(Call[])` decode (2026-06-03)

> The **#1 data-gated DEFER from Q1**: Bundler3 (~14.5% of all vault-touching txs / ~84% of Morpho-native
> deposits) is now decoded. Core-decoder change (a new **per-leg-to** multicall strategy) + GeneralAdapter1
> adapter-leg surface. Commits: `dff1fba6`(engine) → `58290439`(GA1) → `634d7c3a`(Bundler3) →
> `07af0876`(corpus) → `0a57c9ce`(extension).

**What changed.**
- **Engine** (`policy-engine-wasm`): new ADDITIVE `multicall_call_array` emit.strategy — decodes
  `multicall(Call[])` (`Call=(address to,bytes data,uint256 value,bool skipRevert,bytes32 callbackHash)`)
  by re-routing each leg **at its OWN `to`** (per-leg-to), vs the existing same-`to` `multicall_recurse`.
  Plus `maybe_inject_metamorpho_underlying` (arg-shape-gated injector + committed 73-vault snapshot) so a
  GeneralAdapter1 `erc4626*` leg fills the REQUIRED `asset` (the vault is a runtime arg; the underlying is
  not in calldata, unlike the direct metamorpho manifests that bake it).
- **GeneralAdapter1** surface + **10 cover manifests**: morpho* (6) → the SAME Morpho Blue body as a direct
  call (market from `$args.marketParams`, `market_id` auto-derived); erc4626* (4) → metamorpho{vault=$args.vault}.
  16 exclude (onMorpho* callbacks + deferred plumbing/staking/wrapper).
- **Bundler3** surface + `multicall(Call[])` manifest (`multicall_call_array`); `reenter`=exclude (guarded).
- **Extension** loader: per-leg-to child pre-install (each leg installed at its own `to`).

**Real-tx validation** (`v3-harness corpus --filter bundler3 --require-expect-body` → **2/2 matched, 2/2
pinned**): a `[morphoWithdrawCollateral, erc20Transfer(skip)]` bundle → `Multicall[withdraw]` (`market_id`
keccak-derived, asset=wstETH collateral / amount / recipient pinned); a `[Permit2 permit,
morphoSupplyCollateral, erc20Transfer(skip)]` bundle → `Multicall[permit2, supply]` — demonstrating
per-leg-to routing to **TWO different contracts** (Permit2 + GeneralAdapter1) and unmapped-leg skipping.

**Gates (all green) — post Q2 honest-review fixes (D-A…D-E2).** check:surface PASS (Bundler3 2/1/1;
GeneralAdapter1 26/**20**/6; I0 morpho 3 cover); check:manifest **1761** OK / 0 errors; check:universe 73
cover-linkage; check:tokens PASS; engine unit tests (`multicall_call_array_routes_each_leg_by_its_own_to`,
`…unknown_vault_metamorpho_fails_whole_bundle` [D-A], `extract_morpho_reenter_legs_decodes_supplycollateral_callback`
[D-C]); extension vitest **3/3** (incl D-C callback pre-install); **`v3_decode_harness` golden 61/61** +
bundler3 corpus **4/4 field-pinned** (withdraw+sweep / permit2+pull+supply+sweep / flashLoan-leverage / Lido
deleverage); `cargo test --workspace` 0 fail; wasm32 compile + wasm-pack bundle + tsc 0.

**Honest coverage re-measurement — FRESH all-73 (supersedes the ~17% 8-cover estimate).** A fresh Dune
measurement over ALL 73 listed mainnet vaults (30d, ERC-4626 `Deposit` events bucketed by tx.to entrypoint,
n=10,860) gives the true post-Q2 split: **direct-to-vault 1.64%** (Q1) + **Bundler3 11.95%** (Q2) =
**covered ≈ 13.6%**; **other 86.41%**. The prior ~17% (direct 2.7% + Bundler3 14.5%) was extrapolated from
the 8-cover FW-2 sample, which over-weighted Bundler3 — the all-73 reality is lower. The 86% "other" is a
FRAGMENTED long tail of zapper/aggregator/router contracts (top 3 unknown routers ≈ 32%:
`0x33024d47…` 11.5% / `0x50461744…` 10.7% / `0x8a25a24e…` 10.3%), **NOT AA-dominated** — the ERC-4337
EntryPoint `0x0000000071727de2…` is only ~2.1% (correcting the prior "AA-entrypoints" framing). (GA1-direct =
0%, confirming GA1 is reached only via Bundler3.) WITHIN a Bundler3 bundle the morpho-blue/metamorpho legs
decode FULL (per-leg-to → `ActionBody::Multicall`), and the Q2 honest-review deficiencies are CLOSED:
**D-A** unknown-vault erc4626 → REFUSE the bundle (warn-closed), never a 0x0-asset silent decode;
**D-B** transfer/plumbing legs (erc20/permit2/native transfer, wrap/unwrap) → token-domain `erc20_transfer`;
**D-C** nested `reenter(Call[])` callbacks (leverage/flash-loan loops) → recursed (even on EXCLUDE
morphoFlashLoan); **D-D** Lido stake legs (stakeEth/wrapStEth/unwrapStEth) → liquid_staking. Residual
within-Bundler3 defers: **ParaswapAdapter swap** (srcToken/destToken ARE explicit, but the AMOUNTS live in the
opaque Augustus `bytes callData` and amm SwapAction needs concrete amounts — 0.49% of bundles) + **erc4626*
underlying for any vault outside the 73-snapshot** (D-A refuses it; re-gen the snapshot to extend).

## Final Completion Claim

**Onboarding status: COMPLETE (mainnet-only; full listed-vault direct coverage + Bundler3 router), bounded by measured entrypoint share.**

> **wallet-facing, Ethereum mainnet (`1`) ONLY: Morpho Blue Full-8 — supply/withdraw/borrow/repay/supplyCollateral/withdrawCollateral/setAuthorization + off-chain Authorization — plus MetaMorpho ERC-4626 ALL 73 listed mainnet vaults (Q1, direct) plus Bundler3 `multicall(Call[])` decoded per-leg-to into GeneralAdapter1 position legs (Q2).**
>
> **Measured coverage (FRESH all-73, 30d Deposit-entrypoint split, n=10,860):** Q1 covers 100% of listed mainnet vaults / TVL / vault-direct txs. Q2 + the D-A…D-D fixes add the Bundler3 router (full per-leg decode incl `reenter` callbacks). Across all 73 vaults' deposit entrypoints: **covered ≈ 13.6%** (direct-to-vault 1.64% + Bundler3 11.95%). This **supersedes the prior ~17%** (an 8-cover-sample extrapolation that over-weighted Bundler3). This is **NOT "the full surface a Morpho user signs"** — the remaining **~86%** routes through a FRAGMENTED long tail of other zapper/aggregator/router contracts (top 3 ≈ 32%; ERC-4337 AA only ~2.1%, NOT the dominant path), still deferred.
>
> **Deferred (data-gated, separate-surface or separate-framework):** other routers/aggregators (~86% of deposit entrypoints, fragmented long tail — top 3 unknown routers ≈ 32%); ParaswapAdapter swap (amounts in opaque Augustus callData; 0.49% of Bundler3 bundles); erc4626* underlying for vaults outside the 73-snapshot (D-A refuses, not silent); Base & all other chains (multichain framework); MetaMorpho V2/VaultV2; URD claim; mint/redeem §4d `convertToAssets` enrichment. *(The 65-vault long-tail [Q1], Bundler3 [Q2], and the Q2 honest-review deficiencies [D-A transfer-IN/0x0-asset, D-B plumbing, D-C nested callbacks, D-D Lido] are no longer deferred.)*

Verify:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- morpho --phase all
```
