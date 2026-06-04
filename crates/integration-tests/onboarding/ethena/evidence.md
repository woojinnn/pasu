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
| fuzz command with seed recorded | pending | |
| iterations >= 5000 or justified lower bound | pending | |
| fixed edge-case matrix recorded | pending | |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | pending | |
| representative pass/error corpus entries committed or justified | pending | |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | pending | |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | pending | |
| external tx pull target address count is nonzero and recorded | pending | |
| Etherscan `api_calls_used` recorded | pending | |
| Etherscan `raw_txs_seen` recorded | pending | |
| Etherscan `unique_selectors_seen` recorded | pending | |
| Etherscan real tx coverage per COVER selector recorded | pending | |
| wallet-facing target sweep executed or explicitly not applicable | pending | |
| unmatched Etherscan txs classified actionable/non-actionable | pending | |
| pool-heavy/factory swept candidate/universe addresses, or n/a | pending | |
| unknown to-addresses with known protocol selectors bucketed | pending | |
| typed-data signing corpus/golden for every in-scope EIP-712 type, or n/a | pending | |
| Dune MCP/API availability checked | pending | |
| Dune usage baseline recorded | pending | |
| Dune calibration/query executed or blocked | pending | |
| Dune `executionCostCredits` / usage delta recorded | pending | |
| Dune rows returned / selected tx hashes recorded | pending | |
| representative real-tx corpus/golden entries committed or justified | pending | |
| protocol-filtered corpus replay with semantic pin gate | pending | |
| SCOPE ORACLE — covered-surface real-usage coverage-share (H2 volume-weighted, H3 wrapper child-rate), each DEFER usage-share | pending | |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 gaps bucketed | pending | |
| each fix tied to gap id/selector/tx hash/seed | pending | |
| manifest/decoder/Tier3/harness change list recorded | pending | |
| P2 rerun after fixes recorded | pending | |
| corpus `expect` flips or exclusions justified | pending | |
| remaining gaps have explicit defer/blocker disposition | pending | |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | pending | |
| registryV2 build-index vitest output recorded | pending | |
| `npm run check:manifest` output recorded | pending | |
| `npm run check:surface` output recorded | pending | |
| `check:universe --require-cover-linkage` recorded, or n/a | pending | |
| v3-harness coverage/fuzz/corpus outputs recorded | pending | |
| protocol-filtered strict corpus output recorded | pending | |
| `cargo test --workspace` output recorded | pending | |
| wasm build output recorded if runtime/wasm/schema changed | pending | |
| fmt/clippy/typecheck output for changed crates/packages | pending | |
| exact staged files and commit hash recorded | pending | |
| remaining WARNs/deferred selectors/actions listed | pending | |
| final completion label without overclaiming | pending | |
| no base/worktree merge unless user explicitly requested | pending | |

## Blockers

| blocker | source | next action |
|---|---|---|
| (none) | | |

## Final Completion Claim

Pending P1–P4. Gate: `cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- ethena --phase all`.
