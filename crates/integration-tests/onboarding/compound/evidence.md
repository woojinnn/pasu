# Compound Protocol Onboarding Evidence

> Onboarding run for **Compound — all live generations** (v2 cToken, v3 Comet, governance).
> Treat existing `compound-v3` artifacts as candidates, not proof (♻️ re-verify). v2 is greenfield.
> Fill each phase row with exact commands, counts, artifacts, and blockers.
> SSOT for per-phase requirements = `ONBOARDING_EVIDENCE_TEMPLATE.md`; "onboarded" definition = `PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md` §6.

## Run Metadata

| field | value |
|---|---|
| protocol | compound |
| branch | feat/compound-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-compound |
| date | 2026-06-02 |
| main agent | Claude Code (Opus 4.8, 1M) |
| base commit | a8909023 (feat/registry-v2 — re-grounded framework + doc_grounding/check:tokens gates) |

## Scope Classification

User directive: "compound … 실제 사용되는 버전 전부를 온보딩" — onboard **every actually-used Compound version**.
Compound markets are called **directly** by wallets (no router fronts user actions), so "wallet-facing"
here = direct market/Comptroller/Bulker/Rewards calls — these are NOT deferred as factory-children.

| field | value |
|---|---|
| primary chain(s) | v2: Ethereum mainnet (1) only. v3 (Comet): mainnet(1) + Arbitrum(42161) + Base(8453) + Optimism(10) + Polygon(137) + Linea(59144) + Scroll(534352) + Mantle(5000) + Unichain(130) + Ronin(2020). Governance: mainnet(1). [chain set to be 1차-출처 confirmed in P0] |
| completion target | `full-surface` (direct wallet-facing market calls across all live deployments) |
| multichain expansion | included for v3 Comet (already 28 deployments in registry); v2 + governance are mainnet-only by protocol design |
| direct factory-child calls | covered — cToken markets (v2) and Comet markets (v3) are direct user entrypoints; live-market universe enumerated in P0 |
| final claim label | TBD at P4 — will not overclaim |

### Current-state diagnosis (pre-P0, from registry inspection @ a8909023)

- **v3 Comet — PARTIAL (43 manifests, all `comet/`)**: actions `supply{,From,To}` `withdraw{,From,To}` `transfer{,Asset,AssetFrom,From}` `buyCollateral` `approve` `approveThis` `allow` `allowBySig` (15 fns) + 28 `authorization-sign` typed-data + 56 surface abi/coverage snapshots + golden corpus. ♻️ re-verify against current Comet ABI.
- **v3 Comet — GAPS (suspected, P0 to confirm)**: `Bulker.invoke` (native-ETH wrap supply/withdraw — heavy real usage) ❌; `CometRewards.claim`/`claimTo` (COMP rewards) ❌.
- **v2 cToken — ABSENT (greenfield)**: no `compound-v2` manifests. cToken `mint`/`redeem`/`redeemUnderlying`/`borrow`/`repayBorrow`/`repayBorrowBehalf`/`liquidateBorrow` + ERC20 `transfer`/`approve`; Comptroller `enterMarkets`/`exitMarket`/`claimComp`; Maximillion `repayBehalf` (cETH). Mainnet live TVL.
- **Governance — ABSENT**: COMP `delegate`/`delegateBySig`; GovernorBravo `castVote{,WithReason,BySig}`/`propose`/`queue`/`execute`. Scope wallet-facing subset; defer admin-only.
- **Surface gate gap**: no `registryV2/surface/compound-v3/_deployments.json` → `check:surface` reports compound-v3 contract-inventory NOT enforced (I0 not gated). P0 must author `_deployments.json` for the full Compound surface.

### Design decision (recorded)

Compound v2 maps to **existing ActionBody domains** (lending: supply/withdraw/borrow/repay/liquidate; token: cToken ERC20; permission/airdrop: enterMarkets/claimComp). Most likely **no new domain**; possible small Tier-3 **sub-action** additions (e.g. lending liquidate, collateral-toggle, redeem shares-vs-underlying) verified against the `lending` domain in P1. **No up-front ExitPlanMode** unless a genuine new domain emerges during P1 (framework: "새 domain 같은 큰 설계만 plan 1회").

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | See Scope Classification above. Target = full wallet-facing surface, all 3 generations, v3 multichain included. |
| Codex current-session research executed | n/a | This run is Claude-led (no parallel Codex session); independent 2nd opinion satisfied via sub-agent fan-out below. |
| Claude Code or sub-agent research executed | done | 3 sub-agents (v2 surface / v3 Comet gaps / governance+token) + Bulker 1차 fetch via WebFetch (compound-finance/comet BaseBulker.sol 6 tags + MainnetBulker.sol stETH 2). |
| Claude/sub-agent exact prompt or command recorded | done | sub-agent fan-out (P0 progress log); WebFetch BaseBulker/MainnetBulker; `cast sig` all selectors + `cast format-bytes32-string` 8 ACTION_* tags; on-chain `getAllMarkets()`/`underlying()`. |
| Codex-only candidates listed | n/a | no Codex lane this run |
| Claude/sub-agent-only candidates listed | done | 20 cToken markets, 8 v3 selectors, 8 Bulker ACTION_* tags — all 1차-verified, none trusted unverified (sub-agent-missed castVoteWithReasonBySig/proposeBySig caught in ABI). |
| dropped-unverified candidates listed with reason | done | none dropped — every candidate 1차-confirmed (cast / on-chain / GitHub source / Etherscan-verified). |
| final contract inventory verified against first-party sources | done | v2: Comptroller.getAllMarkets()+underlying() on-chain; v3: comet deployments roots.json + Etherscan-verified MainnetBulker(0xa397…)/CometRewards(0x1b0e…). All in surface/_deployments.json. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | market-heavy: v2 = 20 cToken (getAllMarkets); v3 = 28 Comet + MainnetBulker + CometRewards (roots.json). Enumerated in surface/_deployments.json. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | surface/compound-v2/_deployments.json (26) + surface/compound-v3/_deployments.json (30), committed. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | all cover/exclude in _deployments.json with reason; check:surface I0 enforces full inventory triage. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | concrete per-market manifests (cToken + Comet markets are direct entrypoints); static $resolved.compound_v2_underlying / compound_v3_base_asset for asset resolution. |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | cToken + Comet markets ARE the direct user entrypoints — all covered, none deferred as factory-children. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | n/a | market-heavy, not pool-factory; cover universe closed via _deployments.json + check:surface I0/I2 (cover↔manifest co-dependency). |
| token-surface inventory completed or explicitly scoped out | done | 20 cToken yield_receipt (index rebase_form) + cETH native; Comet base/collateral; COMP. check:tokens PASS (0 errors). |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | surface/compound-v2/_deployments.json + surface/compound-v3/_deployments.json (MainnetBulker flipped exclude→cover). |
| `npm run check:surface` output recorded | done | PASS — compound-v2 + compound-v3 I0/I1/I2/S green; MainnetBulker [1] 4 surface·1 cover·3 exclude·1 manifest; I0 compound-v3 30 cover·0 exclude. |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | v2 24 manifests + v3 Comet 43 + Rewards 2 + Bulker 1 (8 per_tag) → existing lending/airdrop/token domains. NO new ActionBody domain/sub-action. |
| permission/fund-movement/red-flag selector review recorded | done | claimTo `to` + Bulker per-leg `to`/`src` = fund-destination; delegate/Delegation = voting-power; liquidate `victim` = liquidation target; no admin/red-flag selectors COVERed (sweep*/transferAdmin/governor-only all exclude). |
| manifest files added/changed listed | done | compound-v2/* (24), compound-v3/bulker/invoke@1.0.0 (8 per_tag), compound-v3/rewards/* (2), + existing comet 43. |
| enrichment/live_field decision recorded for every COVER action | done | all live_inputs = derived_from skeletons (enrichment DORMANT — verdict 100% local WASM Cedar; policy_rpc unwired). Supply 5 / Withdraw 3 / Claim 4 / Delegate 2 fields. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | none required at verdict time (enrichment dormant; live_inputs are static skeletons). No blocker. |
| Tier3 not needed or full Tier3 downstream contract completed | done | NO Tier3 (new ActionBody domain) needed. One DECODER grammar extension: new `emit.strategy: parallel_tagged_dispatch` (additive route arm + helpers in declarative_exports.rs; commit 2d58a1d8) — a decoder-capability edit, not an ActionBody schema change. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | n/a | no Tier3 (new domain) — grammar extension is decoder-only (`crates/policy-engine-wasm/src/declarative_exports.rs`); no ActionBody/effect/lowering_v2/cedarschema touched. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | validate --filter compound-v2 → 147 OK; --filter compound-v3 → 422 OK; check:manifest (all) → 1606 OK, 0 structural errors. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `v3-harness fuzz --filter compound-v2` + `--filter compound-v3` (seed=0x5c09eba1). |
| iterations >= 5000 or justified lower bound | done | compound-v2 44,100 (300/manifest × 147); compound-v3 28,800 (64/callkey). Both ≫ 5000. 0 fail/panic. |
| fixed edge-case matrix recorded | done | array_emit (enterMarkets), parallel_tagged_dispatch empty-array + unknown-tag + length-mismatch (Bulker), native-sentinel + stETH/wstETH-literal assets, payable cETH, claim. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | array (enterMarkets / Bulker batch up to 32), value (cETH/native supply, Maximillion $tx.value), nested (Bulker per-tag ABI tuple), permission (COMP delegate / Comet allow). |
| representative pass/error corpus entries committed or justified | done | compound-v2 26 entries + compound-v3 75 entries (real-tx + synthetic + typed-data); all `expect: pass`. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | ETHERSCAN_API_KEY in crates/integration-tests/.env; api.etherscan.io v2 (chainid=1). |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | Bulker 0xa397… (9,895 invoke txs, 3 pages) + v2 cUSDC/cDAI/cETH/Maximillion/COMP (10k each). Selectors decoded blind, then matched to manifests. |
| external tx pull target address count is nonzero and recorded | done | 7 cover addresses swept (6 v2 + MainnetBulker). |
| Etherscan `api_calls_used` recorded | done | ~16 txlist calls (3 Bulker pages + 5 v2 addrs × ~2 pages + retries). |
| Etherscan `raw_txs_seen` recorded | done | Bulker 9,895 invoke txs; v2 ≈50k txlist rows (10k × 5 addrs). |
| Etherscan `unique_selectors_seen` recorded | done | Bulker: 8 ACTION_* tags across 84 action-combos. v2: mint/borrow/redeem/redeemUnderlying/repayBorrow/repayBehalf/liquidateBorrow/enterMarkets/exitMarket/claimComp/delegate. |
| Etherscan real tx coverage per COVER selector recorded | done | v2 REAL: mint·borrow·redeem·redeemUnderlying·repayBorrow·liquidateBorrow·enterMarkets·exitMarket·claimComp·delegate·repayBehalf(Maximillion). v3 Bulker REAL: 6/8 tags (SUPPLY/WITHDRAW_NATIVE dominant, SUPPLY/WITHDRAW_ASSET, SUPPLY_STETH, CLAIM). **0-sample (synthetic-only):** v2 repayBorrowBehalf, delegateBySig; Bulker WITHDRAW_STETH, TRANSFER_ASSET. |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | Bulker invoke 9,895 raw → bucketed to 84 combos → 5 representative txs selected (per-action). v2: 5 markets, floor = 1 real tx per missing selector. Target list = the surface/_deployments.json cover addresses. |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | every swept tx hit a COVER selector (Bulker = invoke only; v2 markets = cToken/Comptroller selectors, all COVERed or admin-exclude). No unknown actionable selector surfaced. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | n/a | market-heavy, not pool-factory; the cover universe = the enumerated market addresses, all swept directly. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | none — swept by known cover addresses; markets are direct entrypoints (no router fronting). The ONE hard gap (Bulker.invoke grammar) was found in P0 and CLOSED in this slice (parallel_tagged_dispatch). |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | Comet `Authorization` (28 entries, compound-v3) + COMP `Delegation` (1 entry, compound-v2). Both route + pass (`corpus --filter`). cToken: none (no EIP-712). |
| Dune MCP/API availability checked | n/a | mcp__dune__* available but not needed — v2 + v3 Bulker are mainnet-only (one-chain methodology); Etherscan sweep sufficient. |
| Dune usage baseline recorded | n/a | Dune not used (mainnet-only). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | n/a | Dune not used (mainnet-only, Etherscan sufficient). |
| Dune `executionCostCredits` / usage delta recorded | n/a | Dune not used. |
| Dune rows returned / selected tx hashes recorded | n/a | Dune not used. |
| representative real-tx corpus/golden entries committed or justified | done | compound-v3 corpus +5 real Bulker (commit b10819f8); compound-v2 corpus +3 real (mint/liquidate/repayBehalf) + Delegation typed-data (commit d31b5740). |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `corpus --filter compound-v2` → 26/26 (11 pinned); `--filter compound-v3` → 75/75 (5 Bulker pinned, 46 pins). `--require-expect-body` strict gate is blocked ONLY by 70 PRE-EXISTING synthetic v3 entries that predate the convention (not this slice's work); all NEW real-tx entries carry pins. |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | HARD: Bulker.invoke grammar (no per-element-tagged array decode). SOFT: base_asset un-resolvable from $to=Bulker. MISDECODE: 0 (corpus 101/101). Tooling: check-tokens listed `governance` as a top-level kind (it's nested). |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | grammar→`parallel_tagged_dispatch` (selector 0x555029a6, tx 0x85658f8…/0xcdd24cb…); base_asset→`resolve_from_inputs:{compound_v3_base_asset:comet}`; native→native-sentinel; check-tokens→commit b383acb2. |
| manifest/decoder/Tier3/harness change list recorded | done | decoder: build_parallel_tagged_dispatch + derive_tag_key + resolve_named (declarative_exports.rs, 2d58a1d8). manifest: compound-v3/bulker/invoke + surface flip + v2 corpus depth. No Tier3. |
| P2 rerun after fixes recorded | done | post-fix: corpus 101/101 (v2 26 + v3 75); fuzz 28,800 + 44,100; validate 422 + 147; wasm suite 44/44; compound harness 11/11. |
| corpus `expect` flips or exclusions justified | done | Bulker NATIVE asset pin `asset.key.address`→`asset.key.standard` (TokenKey::Native has no address field). enterMarkets `expect_domain`→`multicall` (array_emit wraps per-cToken EnableCollateral; prior v2 slice). |
| remaining gaps have explicit defer/blocker disposition | done | 0-real-sample selectors (repayBorrowBehalf, delegateBySig, WITHDRAW_STETH, TRANSFER_ASSET): synthetic-only (validate/fuzz cover; manifests exist). multichain + GovernorBravo-voting: deferred (see Final Completion Claim / Blockers). |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | build-index: 813 manifests, 3828 tokens, 53,133 callkeys + 85 typed-data written. |
| registryV2 build-index vitest output recorded | n/a | registryV2 has no separate vitest suite — build-index.ts IS the validation (ran clean via check:manifest, 0 errors). |
| `npm run check:manifest` output recorded | done | build-index 813 manifests + validate (all) 1606 single_emit OK, 0 structural errors. |
| `npm run check:surface` output recorded | done | PASS — every gated contract triaged; MainnetBulker [1] 4 surface·1 cover·3 exclude; I0 compound-v3 30 cover·0 exclude. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | n/a | market-heavy, not pool/factory/vault; cover↔manifest linkage enforced by check:surface I2 instead. |
| v3-harness coverage/fuzz/corpus outputs recorded | done | fuzz compound-v2 44,100/44,100 · compound-v3 28,800/28,800 (0 fail/panic); corpus compound-v2 26/26 · compound-v3 75/75. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | compound-v2 26/26 (11 pinned) · compound-v3 75/75 (5 Bulker pinned, 46 pins). Strict `--require-expect-body` fails ONLY on 70 pre-existing synthetic v3 entries lacking pins (predate convention; not this slice). |
| `cargo test --workspace` output recorded | done | `cargo test --workspace -- --test-threads=4` → exit 0, 0 failed / 0 panicked (full suite green; --test-threads=4 to avoid the known FW-1 harness OOM). |
| wasm build output recorded if runtime/wasm/schema changed | done | decoder changed → `./scripts/wasm-build.sh` rebuilt + wasm-opt'd; bundle 10.23 MiB; copied to browser-extension/backend/wasm/ + public/wasm/. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | `cargo clippy -p policy-engine-wasm --all-targets` clean (0 warn after doc-fix); `cargo fmt -p policy-engine-wasm --check` clean; registryV2 `tsc --noEmit` clean; `check:tokens` PASS (0 errors). |
| exact staged files and commit hash recorded | done | 2d58a1d8 (decoder), b10819f8 (Bulker manifest+surface+corpus), d31b5740 (v2 P2 depth). Explicit-stage (no `git add -A`). 7 commits ahead of base feat/registry-v2 @ a8909023, NOT pushed. |
| remaining WARNs/deferred selectors/actions listed with reason | done | 0-real-sample synthetic-only: v2 repayBorrowBehalf/delegateBySig, Bulker WITHDRAW_STETH/TRANSFER_ASSET. Deferred: multichain (one-chain methodology), GovernorBravo voting (low value — see Blockers). check:tokens 1344 pre-existing referential warns (non-strict, not this slice). |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | See Final Completion Claim below. |
| no base/worktree merge performed unless user explicitly requested it | done | no merge/push performed; branch feat/compound-onboarding holds all work locally. |

## Progress log

### compound-v2 slice — P0+P1 DONE, green (2026-06-02)

- **P0 research:** 3 sub-agents (v2 surface / v3 Comet gaps / governance+token), 1차 출처 = compound-finance GitHub + docs + Etherscan-verified. Candidates re-verified against code/on-chain, NOT trusted: selectors via `cast sig` (incl. CEther payable overloads + sub-agent-missed castVoteWithReasonBySig/proposeBySig found in the ABI); cToken market universe via on-chain `Comptroller.getAllMarkets()` (20 markets, authoritative) + each `underlying()` (19 verified on-chain); token_kind schema via the Rust `TokenKind` enum; lending/airdrop domain mapping via the action source.
- **P0 surface:** `surface/compound-v2/_deployments.json` (26 contracts: 20 cToken markets + Comptroller + Maximillion + COMP cover; GovernorBravo/Timelock/GovernorAlpha exclude) + 5 coverage + 5 abi snapshots (verified ABIs). `npm run check:surface` → **PASS** (I0 26 deployed·23 cover·3 exclude; I1/I2/S1/S2 green).
- **Design:** v2 maps entirely to existing `lending` (Supply/Withdraw/Borrow/Repay/Liquidate/Enable+DisableCollateral) + `airdrop` (Claim/Delegate) domains — **no new domain, no new sub-action**. `LendingVenue::CompoundV2 { chain, comptroller }` already existed.
- **Decoder (additive, data-only):** `declarative_exports.rs` `compound_v2_underlying(chain,cToken)` static resolver (19 underlyings on-chain-verified + cETH→native), mirroring `compound_v3_base_asset`; `action_builder.rs` wrap_live_field default `(airdrop,delegate,voting_power)=>"0"` (new action's live-field skeleton).
- **P1 manifests (24):** cerc20 ×7 (mint/redeem/redeemUnderlying/borrow/repayBorrow/repayBorrowBehalf/liquidateBorrow), ceth ×7 (payable overloads, native asset), comptroller ×5 (enterMarkets[array_emit]/exitMarket/claimComp×3), maximillion ×2, comp ×2 (delegate/delegateBySig) + comp Delegation typed-data. `v3-harness validate --filter compound-v2` → **147 single_emit OK, 0 errors**; `fuzz --filter compound-v2 -i 300` → **44100/44100 pass, 0 fail/panic** (exercises array_emit). Enrichment dormant (derived_from skeletons; verdict 100% local).
- **Dogfood gate fix (committed `b383acb2`):** `check-tokens.ts` had `governance` as a valid top-level token_kind (it's a nested `BaseCategory`) — masked 4 malformed ZRO files; gate re-grounded on the Rust enum + 4 ZRO files fixed.

### compound-v3 slice — Rewards + Bulker DONE (2026-06-02)

- `surface/compound-v3/_deployments.json` authored → **I0 now ENFORCED** (30 contracts: 28 Comet + CometRewards + MainnetBulker, all cover). Closes the prior "contract-inventory NOT enforced" WARN for the existing 28 Comet markets.
- **CometRewards (mainnet)** `claim`/`claimTo` → `Airdrop::Claim` (claimTo's arbitrary `to` = fund-destination signal). rewards.coverage.json + abi snapshot + 2 manifests. `validate` 2 OK · `fuzz` 600/600 pass. Mainnet rewards addr on-chain-verified (`rewardConfig`→COMP). `check:surface` PASS. Multichain rewards = decode follow-up.
- **Bulker `invoke` DONE — grammar gap closed (new emit strategy `parallel_tagged_dispatch`, commit `2d58a1d8`).** `invoke(bytes32[] actions, bytes[] data)` is a parallel-array tagged dispatch where each `data[i]` is an action-specific ABI tuple keyed by the `actions[i]` `bytes32` ASCII tag. The new strategy fans out to a `Multicall` of per-element bodies (generalises `tagged_dispatch`→array + `array_emit`→per-element-tag). 1차 source = compound-finance/comet `BaseBulker.sol` (6 tags) + `MainnetBulker.sol` (+stETH 2); 8 `ACTION_*` tags `cast`-verified; selector `0x555029a6`; addr Etherscan-verified MainnetBulker (`0xa397…`).
  - **Decoder (`declarative_exports.rs`):** `build_parallel_tagged_dispatch` + `derive_tag_key` (bytes32_ascii|bytes32_hex|uint) + `resolve_named`. Wiring = ONE route arm + helpers (install strategy-agnostic; harness reuses the export; EmitRule typed enum is test-only — nothing else touched). Posture: fail-loud structural / fail-visible unknown-tag / empty→empty Multicall. e2e unit test green; full wasm suite 44/44; clippy+fmt clean.
  - **base_asset wrinkle (the reason the strategy was needed):** direct comet manifests resolve `base_asset` via `$resolved.compound_v3_base_asset(chain,$to)`, but in a Bulker call `$to` is the Bulker and the market is a per-leg `$inputs.comet`. Solved with the manifest directive `resolve_from_inputs: {compound_v3_base_asset: comet}` — the strategy computes the per-leg `$resolved.*` from a decoded input addr, reusing the existing resolver with no cross-crate duplication. Verified in the unit test (cUSDCv3→USDC) and real-tx corpus.
  - **Manifest** `manifests/compound-v3/bulker/invoke@1.0.0.json` — 8 per_tag bodies mirroring the direct comet/rewards bodies (supply/withdraw/transfer→token-erc20_transfer/claim→airdrop + native-sentinel & stETH/wstETH-literal variants). `_deployments.json` Bulker exclude→**cover** + `bulker.abi.json` (Etherscan-verified, 16 fns) + `bulker.coverage.json` (invoke cover; sweepNativeToken/sweepToken/transferAdmin admin-exclude). `npm run build` + `check:surface` **PASS** (MainnetBulker [1]: 4 surface·1 cover·3 exclude·1 manifest; I0 compound-v3 30 cover·0 exclude) · `validate` 422 OK · `fuzz` 28800/28800 pass.
  - **P2 real-tx (FIRST real-tx entries for compound-v3):** Etherscan `txlist` 0xa397… swept **9,895 invoke txs**; bucketed into 84 action-combos. 5 representative real txs → `corpus.json` (+5 = 75), **75/75 matched**, **46 `expect_body` pins** (per-leg `venue.comet`, `base_asset` resolved-from-comet, `asset`, `amount`u256, recipient/on_behalf_of, claim target/recipient) — all pass. Coverage: **6/8 tags seen in real traffic** (SUPPLY/WITHDRAW_NATIVE dominant, SUPPLY/WITHDRAW_ASSET, SUPPLY_STETH, CLAIM_REWARD). **`ACTION_WITHDRAW_STETH` + `ACTION_TRANSFER_ASSET`: 0 occurrences in 9,895 txs** — modeled from 1차 source, decode-proven via the strategy unit test path + fuzz (synthetic-only; no real sample exists to pin). `--require-expect-body` strict gate reports the 70 PRE-EXISTING synthetic compound-v3 entries (predate the convention; not Bulker-introduced).

### P2 real-tx slice — sampled, green (2026-06-02)

- Etherscan `txlist` (mainnet) for cUSDC / cETH / Unitroller / COMP → 22 REAL txs across the COVER selectors that appeared in recent traffic (redeem, redeemUnderlying, repayBorrow, cETH mint()/redeem/borrow/redeemUnderlying, enterMarkets, exitMarket, claimComp, delegate). Corpus = `data/golden/v3-decode/compound-v2/corpus.json`.
- `v3-harness corpus --filter compound-v2` → **22/22 matched** (all real calldata decodes to the expected domain). **8 entries carry `expect_body` field-pins** (lending: venue.name=compound_v2, asset.key.address=USDC, amount via `u256_hex_eq`; airdrop: token=COMP, delegatee/recipient hex_eq) — all pass → semantic-level verification against real calldata.
- **P2 finding (legit, fixed):** `enterMarkets([cTokens])` decodes to top-level **`multicall`** (one `EnableCollateral` per cToken), NOT `lending` — the decoder is correct (array_emit wraps per-element bodies in a multicall); the corpus `expect_domain` was corrected to `multicall`.

### Remaining (not yet done this run)
- **P2 depth (v2):** ✅ deepened (2026-06-03). Swept ~50k txs (cUSDC/cDAI/cETH/Maximillion/COMP, 10k each) for selectors absent from the sampled corpus. Added 3 REAL-tx + 1 typed-data → compound-v2 corpus (26/26 matched): **cUSDC `mint`** (the primary supply op; 660/1241 hits cUSDC/cDAI — was a real gap), **cUSDC `liquidateBorrow`** (→lending Liquidate, debt=USDC/collat=cToken/victim), **Maximillion `repayBehalf`** (→lending Repay native, amount=$tx.value, on_behalf_of; 9878 hits), **COMP `Delegation`** typed-data (EIP-712 → airdrop Delegate). 15 new expect_body pins, all pass. `borrow` was already covered. **`repayBorrowBehalf` + `delegateBySig`: 0 occurrences in the 50k-tx window** — rare on-behalf/gasless variants; manifests exist + validate/fuzz cover them (synthetic-only, like the Bulker's WITHDRAW_STETH/TRANSFER_ASSET). Dune cross-chain N/A (v2 mainnet-only by protocol design).
- compound-v3 **Bulker** structured decode ✅ DONE (parallel_tagged_dispatch grammar extension, 9,895-tx real corpus). Remaining: **multichain** Bulker/Rewards (mainnet done; Bulker is mainnet-only by deploy anyway — Base/Arb use their own bulkers, decode-follow-up); WITHDRAW_STETH/TRANSFER_ASSET have no real mainnet samples (synthetic-only).
- **GovernorBravo** voting (castVote/propose) — deferred (no governance-vote ActionBody domain; COMP voting-power delegation IS covered).
- **P3** gap triage, **P4** land (`cargo test --workspace` + wasm-build + `check-onboarding-evidence --phase all`).

## Blockers

| blocker | source | next action |
|---|---|---|
| multichain Bulker/Rewards (non-mainnet) | one-representative-chain methodology | DEFERRED — onboarding scope = one representative chain (mainnet). MainnetBulker is mainnet-only by deploy; other chains run their own bulkers. CometRewards/Comet already multichain in the registry; decode-follow-up is a separate multichain framework pass, not single-protocol onboarding. |
| GovernorBravo voting (castVote/propose/queue/execute) | no governance-vote ActionBody domain | DEFERRED (cold-judgment LOW value) — would need a Tier-3 new ActionBody domain, but castVote/propose move no funds and delegate no permission; COMP voting-POWER delegation (the security-relevant primitive) IS covered (delegate + Delegation typed-data). Not worth a new domain for zero fund/permission signal. |
| v2 repayBorrowBehalf, delegateBySig; Bulker WITHDRAW_STETH, TRANSFER_ASSET | 0 real-tx samples in the swept window | NOT a blocker — manifests exist; decode-proven by validate + fuzz (synthetic). No real on-chain sample exists to pin (rare on-behalf/gasless/stETH-withdraw/bulker-transfer variants). |

## Final Completion Claim

**Compound onboarded — mainnet, all live generations, gate-verified (2026-06-03).** Scope: **v2** (20 cToken markets + Comptroller + Maximillion + COMP, full mainnet surface) + **v3** (28 Comet markets + CometRewards + **MainnetBulker `invoke`**, mainnet) + **governance-delegation** (COMP `delegate`/`delegateBySig`/`Delegation` typed-data). The one genuine grammar gap (Bulker parallel-array tagged dispatch) was CLOSED with a new additive `parallel_tagged_dispatch` emit strategy — no new ActionBody domain needed.

**NOT claimed:** full multichain (mainnet only — non-mainnet Bulker/Rewards decode deferred); GovernorBravo on-chain voting (deferred, low value); 4 rare selectors are synthetic-only (no real sample). This is **wallet-facing mainnet full-surface**, not full-universe/multichain.

Every mandatory P0/P1/P2/P3/P4 row above is `done`, `n/a`, or carries a concrete deferral; verified green by:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- compound --phase all
```
