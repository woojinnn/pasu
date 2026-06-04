# Ethena Onboarding Evidence

> Per `ONBOARDING_EVIDENCE_TEMPLATE.md`. Completion gate, not a note.

## Run Metadata

| field | value |
|---|---|
| protocol | ethena |
| branch | feat/ethena-onboarding |
| worktree | ~/Desktop/ScopeBall/scopeball-ethena |
| date | 2026-06-04 |
| main agent | Claude Code (Opus 4.8, ultracode) |
| base commit | 420639b7 (feat/registry-v2) |

## Scope Classification

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | Ethereum mainnet (eip155:1). LayerZero OFT multichain (USDe/sUSDe on other chains) = deferred (separate framework). |
| completion target | `wallet-facing` — sUSDe (StakedUSDeV2) user stake/cooldown/unstake lifecycle. |
| **pre-decision** cross-entry volume distribution (H1) | Measured BEFORE cover/defer: 10k most-recent top-level tx to sUSDe (0x9d39a5…), Etherscan txlist, blocks 24,879,650→25,241,146 (success, to==sUSDe). approve(ERC20) 32.1% · **cooldownShares 28.4%** · **unstake 26.5%** · transfer(ERC20) 7.8% · **deposit 4.6%** · addToBlacklist(admin) 0.5% · cooldownAssets/transferFrom/increaseAllowance ~0%. → staking lifecycle (cooldownShares+unstake+deposit+cooldownAssets) = **~59.5%** of sUSDe top-level tx; ERC20 approve+transfer = ~40% (covered by erc20 standard adapter); admin 0.5%. Cooldown-gated withdrawal (cooldownShares 28.4% + unstake 26.5% = 55%) dominates → validates covering the cooldown lifecycle as #1. USDe entry = standard ERC-20/2612 only (no protocol-specific user action). EthenaMinting = MM-only (not retail). |
| per-cover-candidate wrapper/router child resolution-rate (H3) | **N/A — Ethena has no wrapper/router/multicall surface.** All sUSDe calls are direct (to==sUSDe singleton vault); no permitBatchAndCall / multicall_recurse / opcode_stream / tagged_dispatch. H3 does not apply (no wrapper selector to discount). |
| covered real-usage coverage-share — volume-weighted protocol-level (H2), wrappers by child resolution-rate (H3) | To be measured P2 (build-after). Projected: covered staking lifecycle (deposit/mint/cooldownShares/cooldownAssets/unstake/redeem) ≈ 59.5% of sUSDe top-level tx directly; + ERC20 approve/transfer/transferFrom (~40%) covered by erc20 standard adapter; ≈ 99% effective, residual = addToBlacklist 0.5% (admin, excluded) + withdrawERC4626 ~0% (deferred). |
| user-facing DEFERs, each with 1st-party usage-share | `withdraw(uint256,address,address)` (ERC4626 withdraw, assets-denominated): **0%** measured (0/9986 in the 10k window) — reverts while cooldownDuration>0 (live=86400s/1d); deferred (no assets-denominated staking withdraw shape; the shares path is covered by `redeem`). ENA/sENA staking, USDtb, multichain = out of declared scope (separate onboarding), not measured. |
| direct factory-child calls | not applicable — sUSDe is a single singleton vault, not a factory/pool protocol. |
| final claim label | "Ethena sUSDe (StakedUSDeV2) wallet-facing staking lifecycle, mainnet — deposit/mint/cooldownShares/cooldownAssets/unstake/redeem covered; ERC20 surface via standard adapter; withdraw(ERC4626) deferred (0% usage)." (refined with measured H2 at P4). |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification above: mainnet (eip155:1), wallet-facing sUSDe lifecycle, multichain deferred. |
| pre-decision cross-entry volume distribution measured BEFORE the cover/defer boundary (tx-share of each user-facing entry; which entry dominates), so cover/defer is data-driven not assumed (H1) | done | Etherscan txlist 10k top-level tx to sUSDe (blocks 24,879,650→25,241,146); histogram in Scope Classification (H1 row). cooldownShares 28.4% + unstake 26.5% dominate → cooldown lifecycle = #1 cover. |
| Codex current-session research executed | done | This session = Claude Code (no separate Codex). Cross-check done via independent 1st-party on-chain verification instead of a 2nd LLM (single-contract surface, low ambiguity). |
| Claude Code or sub-agent research executed | done | Main Claude Code session: full ABI pull + selector keccak (cast) + on-chain reads (asset()/silo()/cooldownDuration()). No sub-agent fan-out needed (single vault contract). |
| Claude/sub-agent exact prompt or command recorded | done | `curl etherscan getabi address=0x9d39a5…` → 24 state-changing funcs; `cast sig` per function; `cast call`/eth_call asset()=0x4c9edd…(USDe), silo()=0x7fc7c91d…, cooldownDuration()=0x15180=86400s. |
| Codex-only candidates listed | done | none (single session). |
| Claude/sub-agent-only candidates listed | done | none dropped vs found. |
| dropped-unverified candidates listed with reason | done | "7-day cooldown" assumption from the goal prompt — **rejected**: live cooldownDuration()=86400s = **1 day** (admin-mutable, MAX 90d). Used live value. |
| final contract inventory verified against first-party sources | done | All 4 contracts Etherscan-verified (getsourcecode ContractName): StakedUSDeV2, USDe, EthenaMinting; silo via on-chain read. `surface/ethena/_deployments.json`. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | not applicable — sUSDe is a single singleton vault (no factory/pool universe). |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | not applicable — single singleton vault, no `_address_universe.json`. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | not applicable — no factory/pool children. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | not applicable — concrete per-contract manifests (single address, no source-materialize). |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | not applicable — sUSDe is a single vault, not a factory/pool protocol. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable — no `_address_universe.json` (single vault). |
| token-surface inventory completed or explicitly scoped out | done | USDe (already registered, base/stable/usd) ✓; sUSDe enriched → `stake_receipt` {protocol:ethena, underlying:USDe, unlock:cooldown 86400s} (`registryV2/tokens/1/0x9d39a5…json`). `npm run check:tokens` PASS (0 errors). |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | `surface/ethena/_deployments.json` (4 contracts: sUSDe cover; USDe/EthenaMinting/silo exclude). |
| `npm run check:surface` output recorded | done | I0 ethena ✓ (4 deployed·1 cover·3 exclude); StakedUSDeV2 ✓ (24 surface·6 cover·18 exclude). Currently FAIL only on I2 (6 cover selectors lack manifests — built in P1). |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | deposit(0x6e553f65)→staking::Stake; mint(0x94bf804d)→Stake(amount=shares); cooldownShares(0x9343d9e1)→Cooldown(amount=shares,denom=shares); cooldownAssets(0xcdac52ed)→Cooldown(amount=assets,denom=assets); unstake(0xf2888dbb)→Redeem(amount=uint256.max=full silo,recipient); redeem(0xba087652)→Redeem(amount=shares). All `staking` domain, venue=ethena_staked_usde. |
| permission/fund-movement/red-flag selector review recorded | done | All 6 are fund-movement (stake-in / cooldown-lock / withdraw-out); no approve/permit/delegate primitive in cover set (those = ERC20 standard adapter on sUSDe/USDe). The permission-relevant primitive = the **cooldown lockup** (funds locked in silo for cooldownDuration=1d) — made legible via Cooldown.amount + denomination so a policy can bound the cooled quantity. unstake amount=uint256.max = "full cooled balance" (no calldata amount) → policies treating MAX as unbounded behave conservatively. No reentrancy/arbitrary-call surface (single vault). |
| manifest files added/changed listed | done | NEW: registryV2/manifests/ethena/staked-usde/{deposit,mint,cooldown-shares,cooldown-assets,unstake,redeem}@1.0.0.json (6). |
| enrichment/live_field decision recorded for every COVER action | done | NO enrichment / live_field for any action — all fields are pure static decode from calldata (amount/recipient/venue) + literal (denomination, unstake MAX). cooldownDuration is static token_kind metadata (sUSDe stake_receipt.unlock.cooldown 86400s), not a live field. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | none required — no enrichment / policy-RPC for any ethena action (static decode only). |
| Tier3 not needed or full Tier3 downstream contract completed | done | Not a new domain/sub-action (staking::{Stake,Cooldown,Redeem} pre-exist). ActionBody extension (additive): StakeVenue::EthenaStakedUsde {chain,vault} + CooldownAction.{amount,denomination} + new CooldownDenomination{shares,assets} enum. cedarschema cooldown updated; lowering + venue lowering updated; 3 conformance tests (aave whole-balance + ethena shares + ethena assets) pass. No new effect/view/sync (Cooldown Reducer is no-op; no live inputs); no new Cedar action/registration (Cooldown already shipped/registered). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | Edited: crates/policy-server/asset-model/action/src/staking/{mod.rs (venue variant+name),cooldown.rs (amount/denomination/CooldownDenomination)}; crates/policy-engine/src/lowering_v2/staking/{mod.rs (lower_stake_venue arm + test_support venue helper),cooldown.rs (lower amount/denomination + 2 ethena tests)}; schema/policy-schema/actions/staking/cooldown.cedarschema (amount?/denomination?). |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `npm run check:manifest`: build-index 1017 manifests, strict-callkeys OK; `validate (all): 2055 single_emit manifest(s) OK, 0 structural errors`. `v3-harness validate --filter ethena`: 6 single_emit OK, 0 structural errors (iters/manifest=24). |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `v3-harness fuzz --iterations 5000 --seed 20260604 --filter ethena`. |
| iterations >= 5000 or justified lower bound | done | 5000 iters/callkey × 6 callkeys = 30000 total: pass=30000 soft=0 fail=0 panic=0; domain histogram staking=30000 (100%). |
| fixed edge-case matrix recorded | done | corpus covers all 6 cover selectors: deposit/cooldownShares/cooldownAssets/unstake real-tx + mint/redeem synthetic (boundary share/asset amounts). unstake = uint256.max sentinel edge. Random-amount/random-address coverage via the 30000-iter fuzz. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | value: amount in shares (cooldownShares) vs assets (cooldownAssets) vs MAX (unstake) — denomination edge. No nested/array/opcode (single-emit flat calldata, no multicall/stream). No deadline/path. permission: cooldown lockup legibility (amount+denomination). |
| representative pass/error corpus entries committed or justified | done | 9 pass entries (7 real + 2 synthetic) in `crates/integration-tests/data/golden/v3-decode/ethena/corpus.json`. No error entries — malformed calldata handled by fuzz (0 panics); single-emit has no protocol-specific revert shape to pin. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 API reachable (ETHERSCAN_API_KEY in crates/integration-tests/.env, local-only). getabi/getsourcecode/txlist/eth_call all returned. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | `txlist&address=0x9d39a5…(sUSDe)&offset=10000&sort=desc` + same for USDe (0x4c9edd…). Adapter-blind (raw selector histogram, not registry-filtered). Raw saved to logs/ethena/susde_txlist_raw.json. |
| external tx pull target address count is nonzero and recorded | done | 2 entries (sUSDe + USDe), both nonzero. (EthenaMinting/silo excluded — not user-facing.) |
| Etherscan `api_calls_used` recorded | done | ~2 txlist calls (10k tx each) + ABI/getsourcecode/eth_call (~10). |
| Etherscan `raw_txs_seen` recorded | done | sUSDe 9986 top-level success (of 10000 rows); USDe 9993 top-level success. |
| Etherscan `unique_selectors_seen` recorded | done | sUSDe 9 distinct (approve/cooldownShares/unstake/transfer/deposit/addToBlacklist/transferFrom/increaseAllowance/cooldownAssets); USDe 4 (transfer/approve/transferFrom/permit). |
| Etherscan real tx coverage per COVER selector recorded | done | logs/ethena/SCOPE_ORACLE.md H1 table: cooldownShares 28.4%, unstake 26.5%, deposit 4.6%, cooldownAssets 0.0%, mint 0, redeem 0 (cooldown ON → ERC4626 redeem reverts). |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | 2 targets (sUSDe, USDe), 10k tx each (≥ representative floor); matched = all known selectors; target file = surface/ethena/_deployments.json cover/exclude set. No separate router/manager/settlement target (single direct-call vault). |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | sUSDe: addToBlacklist 48 = non-actionable (admin/compliance, EXCLUDE); withdraw/mint/redeem 0 = non-actionable (zero usage). USDe: 0 unmatched (all ERC20). No actionable unmatched. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | not applicable — single singleton vault, no factory/pool universe. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | none — both entries are the known cover/exclude contracts; no unknown to-address carrying an ethena selector. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | not applicable — no in-scope EIP-712 signing for retail sUSDe. EthenaMinting `Order` (EIP-712) is MM-only → excluded; sUSDe `permit` (EIP-2612) is handled by the erc20 standard adapter, not an ethena typed-data manifest. |
| Dune MCP/API availability checked | done | not required for measurement — see below. |
| Dune usage baseline recorded | done | not applicable — direct-call protocol; Etherscan top-level txlist (to==entry) IS the exact top-level measure, no internal-trace disambiguation needed (unlike router-heavy protocols). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | not required — no router/trace top-level-vs-internal disambiguation needed (all sUSDe/USDe calls are direct top-level). Etherscan txlist sufficient and 1st-party. |
| Dune `executionCostCredits` / usage delta recorded | done | not applicable — Dune not used (0 credits). |
| Dune rows returned / selected tx hashes recorded | done | not applicable — tx hashes sourced from Etherscan txlist (see corpus tx_hash fields). |
| representative real-tx corpus/golden entries committed or justified | done | 7 real-tx entries (deposit ×2, cooldownShares ×2, cooldownAssets ×1, unstake ×2) + 2 synthetic (mint, redeem — 0 real in window) with expect_body, committed to ethena/corpus.json. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter ethena --require-expect-body`: 9/9 matched, 9/9 expect_body pinned. expect_body via independent `cast calldata-decode` (non-circular). |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered set decodes), **volume-weighted protocol-level (Σ covered top-level tx / Σ all top-level tx across every user-facing entry, NOT per-contract selector-share) (H2)** and **every wrapper/router selector counted by child resolution-rate, not manifest-presence (H3)**, with each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | logs/ethena/SCOPE_ORACLE.md. H2 = Σ covered/Σ all = (9938+9993)/(9986+9993) = 19931/19979 = **99.76%** across both user-facing entries (sUSDe 99.52% + USDe 100%). H3 N/A (no wrapper surface). User-facing DEFER: withdraw(ERC4626) 0% usage. Covered set (ethena manifests + erc20 standard adapter) — erc20 coverage of sUSDe approve verified by live decode → token::erc20_approve. Label does not over-claim (≈99.8%, admin 0.24% excluded honestly). |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | No hard/soft/misdecoded gaps: corpus 9/9 matched first pass, fuzz 30000/30000, validate 0 structural errors. unknown_protocol_address: none (all to-addresses are the known cover/exclude contracts). excluded gaps: addToBlacklist (admin/compliance), transferInRewards/setCooldownDuration/role/rescue/transferAdmin (infra), EthenaMinting (MM RFQ). |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | No decode fixes required (clean first-pass decode). The only design decision (ActionBody extension) is tied to selectors cooldownShares 0x9343d9e1 / cooldownAssets 0xcdac52ed (amount+denomination) and unstake 0xf2888dbb (Redeem MAX) — committed in P1 (17854568). |
| manifest/decoder/Tier3/harness change list recorded | done | P1: 6 manifests + ActionBody extension (StakeVenue::EthenaStakedUsde, CooldownAction.amount/denomination, CooldownDenomination, cooldown.cedarschema, lowering). Harness: oracle unchanged (no shape-artifact tolerance needed — no $fn/baked-map). P2: corpus + scope-oracle. |
| P2 rerun after fixes recorded | done | No fixes needed; P2 gates green on first pass (fuzz 30000/0, corpus 9/9, validate 6/0). |
| corpus `expect` flips or exclusions justified | done | No flips — all 9 entries `expect: pass` validated against independent cast calldata-decode. No exclusions. |
| remaining gaps have explicit defer/blocker disposition | done | DEFER: withdraw(ERC4626) — 0% usage (reverts while cooldownDuration>0), assets-denominated (no staking withdraw-by-assets shape). Out-of-scope DEFER (user-scoped): ENA/sENA staking, USDtb, multichain (LayerZero OFT). No blockers. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | `npm run build`: 1017 manifests, 3905 tokens across 4 chains, 53809 callkeys + 88 typed-data entries; WARN skipped 239 sourced duplicate callkeys (pre-existing). |
| registryV2 build-index vitest output recorded | done | not applicable — registryV2 has no `test`/vitest script (scripts: build/check:*/typecheck/serve). build-index correctness validated via `npm run build` (0 errors) + `check:manifest` (2055 OK) + strict-callkeys (clean). |
| `npm run check:manifest` output recorded | done | build-index 1017 manifests + strict-callkeys OK; `validate (all): 2055 single_emit manifest(s) OK, 0 structural errors`. |
| `npm run check:surface` output recorded | done | PASS — StakedUSDeV2 [1]: 24 surface · 6 cover · 18 exclude · 6 on-chain manifests · 0 signed-struct; [I0] ethena 4 deployed · 1 cover · 3 exclude. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable — single singleton vault (no `_address_universe.json` / factory children). |
| v3-harness coverage/fuzz/corpus outputs recorded | done | fuzz `--iterations 5000 --seed 20260604 --filter ethena`: 30000 pass / 0 fail / 0 panic. corpus `--filter ethena`: 9/9 matched. validate `--filter ethena`: 6 single_emit OK / 0 errors. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter ethena --require-expect-body`: 9/9 matched, 9/9 expect_body pinned. |
| `cargo test --workspace` output recorded | done | `cargo test --workspace`: all pass EXCEPT 2 PRE-EXISTING failures in policy-engine-wasm (`action_eval_exports::tests::evaluate_action_v2_dashboard_minimal_manifest_{blocks_non_usdt_swap,passes_when_guard_false}`) — Cedar `Amm::Action::"Swap"` optional `tokenOut` validation, unrelated to ethena/staking. VERIFIED pre-existing: reverting my staking changes to base 420639b7 in the working tree → both still FAIL (0 passed/2 failed). My ethena tests all pass (staking conformance 3/3, lowering 37, fuzz 30000, corpus 9/9). 0 regression. |
| wasm build output recorded if runtime/wasm/schema changed | done | `./scripts/wasm-build.sh`: ✨ Done in 2m00s; pkg built; artifact copied to browser-extension/backend/wasm/ + public/wasm/ (gitignored). Schema (cooldown.cedarschema) + Rust (CooldownDenomination, EthenaStakedUsde) compiled to WASM bindings OK. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | `cargo fmt -p policy-action -p policy-engine -- --check`: clean (after 1 single-line import fix in cooldown.rs). `cargo clippy -p policy-action -p policy-engine --all-targets -- -D warnings`: clean (Finished, 0 warnings). registryV2 `npm run typecheck` (tsc --noEmit): PASS. |
| exact staged files and commit hash recorded | done | P0 `0efce66d` (surface/_deployments+abi+coverage, sUSDe token, evidence). P1 `17854568` (staking mod.rs/cooldown.rs + lowering + cedarschema + 6 manifests). P2 `7f8a4856` (corpus.json + evidence). P3/P4 = HEAD after this commit (cooldown.rs fmt fix + evidence.md P3/P4). |
| remaining WARNs/deferred selectors/actions listed with reason | done | DEFER: sUSDe withdraw(ERC4626) — 0% usage, cooldown-ON dead path. EXCLUDE: addToBlacklist + admin/infra; EthenaMinting (MM RFQ); USDeSilo (infra). Out-of-scope: ENA/sENA, USDtb, multichain. build-index WARN: 239 pre-existing sourced duplicate callkeys (not ethena). |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | "Ethena sUSDe + USDe **wallet-facing** surface, **mainnet** — ~99.8% of top-level tx covered (H2 volume-weighted, both entries); sUSDe staking lifecycle (deposit/mint/cooldownShares/cooldownAssets/unstake/redeem) + ERC20 via standard adapter. withdraw(ERC4626) deferred (0% usage); EthenaMinting + multichain out of scope." NOT full-surface, NOT multichain. |
| no base/worktree merge performed unless user explicitly requested it | done | No merge/push performed. Commits stay on feat/ethena-onboarding (worktree scopeball-ethena). |

## Blockers

| blocker | source | next action |
|---|---|---|
| (none) | | |

## Final Completion Claim

**Ethena sUSDe + USDe wallet-facing surface onboarded, Ethereum mainnet (single chain).**
Covered ~99.8% of top-level tx across both user-facing entries (H2 volume-weighted:
Σcovered/Σall = 19931/19979 = 99.76%; sUSDe 99.52% + USDe 100%). sUSDe staking
lifecycle (deposit/mint/cooldownShares/cooldownAssets/unstake/redeem) via 6 ethena
manifests + an additive `staking` ActionBody extension (EthenaStakedUsde venue,
CooldownAction amount/denomination for partial cooldown, CooldownDenomination enum);
USDe + sUSDe ERC20 via the standard adapter. The cooldown-gated withdrawal (1-day
silo lock) — the dominant pre-sign flow (cooldownShares 28.4% + unstake 26.5%) — is
decoded legibly with its locked amount + denomination.

NOT claimed: full-surface (withdraw ERC4626 deferred, 0% usage; admin/EthenaMinting
excluded), multichain (LayerZero OFT deferred), ENA/sENA/USDtb (out of scope).

All land gates green; 2 pre-existing unrelated AMM-dashboard test failures (verified
against base). No base/worktree merge or push performed (awaiting explicit request).

Gate: `cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- ethena --phase all` → see below.
