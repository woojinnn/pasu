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
| Claude Code or sub-agent research executed | pending | |
| Claude/sub-agent exact prompt or command recorded | pending | |
| Codex-only candidates listed | n/a | no Codex lane this run |
| Claude/sub-agent-only candidates listed | pending | |
| dropped-unverified candidates listed with reason | pending | |
| final contract inventory verified against first-party sources | pending | |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | pending | Compound is market-heavy (v2 cToken markets, v3 Comet markets) — enumerate live-market universe per generation |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | pending | |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | pending | |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | pending | |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | pending | |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | pending | |
| token-surface inventory completed or explicitly scoped out | pending | cTokens (v2 receipts → yield_receipt), Comet base+collateral assets, COMP |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | pending | MUST author — currently absent (I0 not enforced) |
| `npm run check:surface` output recorded | pending | |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | pending | |
| permission/fund-movement/red-flag selector review recorded | pending | |
| manifest files added/changed listed | pending | |
| enrichment/live_field decision recorded for every COVER action | pending | |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | pending | |
| Tier3 not needed or full Tier3 downstream contract completed | pending | |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | pending | |
| `npm run check:manifest` or protocol-filtered validate output recorded | pending | |

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
| Etherscan MCP/API availability checked | pending | ETHERSCAN_API_KEY present in crates/integration-tests/.env |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | pending | |
| external tx pull target address count is nonzero and recorded | pending | |
| Etherscan `api_calls_used` recorded | pending | |
| Etherscan `raw_txs_seen` recorded | pending | |
| Etherscan `unique_selectors_seen` recorded | pending | |
| Etherscan real tx coverage per COVER selector recorded | pending | |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | pending | |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | pending | |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | pending | |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | pending | |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | pending | v3 `authorization-sign` (Comet allowBySig EIP-712); v2 COMP `delegateBySig`; cToken none |
| Dune MCP/API availability checked | pending | mcp__dune__* tools available |
| Dune usage baseline recorded | pending | |
| Dune calibration/query executed with partition WHERE or explicitly blocked | pending | |
| Dune `executionCostCredits` / usage delta recorded | pending | |
| Dune rows returned / selected tx hashes recorded | pending | |
| representative real-tx corpus/golden entries committed or justified | pending | |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | pending | |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | pending | |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | pending | |
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
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | pending | |
| v3-harness coverage/fuzz/corpus outputs recorded | pending | |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | pending | |
| `cargo test --workspace` output recorded | pending | |
| wasm build output recorded if runtime/wasm/schema changed | pending | |
| fmt/clippy/typecheck output recorded for changed crates/packages | pending | |
| exact staged files and commit hash recorded | pending | |
| remaining WARNs/deferred selectors/actions listed with reason | pending | |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | pending | |
| no base/worktree merge performed unless user explicitly requested it | pending | |

## Progress log

### compound-v2 slice — P0+P1 DONE, green (2026-06-02)

- **P0 research:** 3 sub-agents (v2 surface / v3 Comet gaps / governance+token), 1차 출처 = compound-finance GitHub + docs + Etherscan-verified. Candidates re-verified against code/on-chain, NOT trusted: selectors via `cast sig` (incl. CEther payable overloads + sub-agent-missed castVoteWithReasonBySig/proposeBySig found in the ABI); cToken market universe via on-chain `Comptroller.getAllMarkets()` (20 markets, authoritative) + each `underlying()` (19 verified on-chain); token_kind schema via the Rust `TokenKind` enum; lending/airdrop domain mapping via the action source.
- **P0 surface:** `surface/compound-v2/_deployments.json` (26 contracts: 20 cToken markets + Comptroller + Maximillion + COMP cover; GovernorBravo/Timelock/GovernorAlpha exclude) + 5 coverage + 5 abi snapshots (verified ABIs). `npm run check:surface` → **PASS** (I0 26 deployed·23 cover·3 exclude; I1/I2/S1/S2 green).
- **Design:** v2 maps entirely to existing `lending` (Supply/Withdraw/Borrow/Repay/Liquidate/Enable+DisableCollateral) + `airdrop` (Claim/Delegate) domains — **no new domain, no new sub-action**. `LendingVenue::CompoundV2 { chain, comptroller }` already existed.
- **Decoder (additive, data-only):** `declarative_exports.rs` `compound_v2_underlying(chain,cToken)` static resolver (19 underlyings on-chain-verified + cETH→native), mirroring `compound_v3_base_asset`; `action_builder.rs` wrap_live_field default `(airdrop,delegate,voting_power)=>"0"` (new action's live-field skeleton).
- **P1 manifests (24):** cerc20 ×7 (mint/redeem/redeemUnderlying/borrow/repayBorrow/repayBorrowBehalf/liquidateBorrow), ceth ×7 (payable overloads, native asset), comptroller ×5 (enterMarkets[array_emit]/exitMarket/claimComp×3), maximillion ×2, comp ×2 (delegate/delegateBySig) + comp Delegation typed-data. `v3-harness validate --filter compound-v2` → **147 single_emit OK, 0 errors**; `fuzz --filter compound-v2 -i 300` → **44100/44100 pass, 0 fail/panic** (exercises array_emit). Enrichment dormant (derived_from skeletons; verdict 100% local).
- **Dogfood gate fix (committed `b383acb2`):** `check-tokens.ts` had `governance` as a valid top-level token_kind (it's a nested `BaseCategory`) — masked 4 malformed ZRO files; gate re-grounded on the Rust enum + 4 ZRO files fixed.

### compound-v3 slice — Rewards DONE + Bulker deferred (2026-06-02)

- `surface/compound-v3/_deployments.json` authored → **I0 now ENFORCED** (30 contracts: 28 Comet + CometRewards cover, Bulker exclude). Closes the prior "contract-inventory NOT enforced" WARN for the existing 28 Comet markets.
- **CometRewards (mainnet)** `claim`/`claimTo` → `Airdrop::Claim` (claimTo's arbitrary `to` = fund-destination signal). rewards.coverage.json + abi snapshot + 2 manifests. `validate` 2 OK · `fuzz` 600/600 pass. Mainnet rewards addr on-chain-verified (`rewardConfig`→COMP). `check:surface` PASS. Multichain rewards = decode follow-up.
- **Bulker `invoke` DEFERRED — hard decode gap (recorded):** `invoke(bytes32[] actions, bytes[] data)` is a parallel-array tagged dispatch where each `data[i]` is an action-specific ABI tuple keyed by the `actions[i]` bytes32 tag. Not expressible by the current emit grammar (`array_emit` = one body per element; `tagged_dispatch`/`enum_tagged_dispatch` = single dispatch; no per-element ABI decode keyed by a per-element tag). Needs an **array + per-element-tagged-dispatch grammar extension** (focused decoder work) to fan out to a multicall of supply/withdraw/transfer/claim/native-wrap/stETH sub-bodies. The underlying ops ARE covered when called directly on Comet/CometRewards; excluded in `_deployments.json` with this reason.

### P2 real-tx slice — sampled, green (2026-06-02)

- Etherscan `txlist` (mainnet) for cUSDC / cETH / Unitroller / COMP → 22 REAL txs across the COVER selectors that appeared in recent traffic (redeem, redeemUnderlying, repayBorrow, cETH mint()/redeem/borrow/redeemUnderlying, enterMarkets, exitMarket, claimComp, delegate). Corpus = `data/golden/v3-decode/compound-v2/corpus.json`.
- `v3-harness corpus --filter compound-v2` → **22/22 matched** (all real calldata decodes to the expected domain). **8 entries carry `expect_body` field-pins** (lending: venue.name=compound_v2, asset.key.address=USDC, amount via `u256_hex_eq`; airdrop: token=COMP, delegatee/recipient hex_eq) — all pass → semantic-level verification against real calldata.
- **P2 finding (legit, fixed):** `enterMarkets([cTokens])` decodes to top-level **`multicall`** (one `EnableCollateral` per cToken), NOT `lending` — the decoder is correct (array_emit wraps per-element bodies in a multicall); the corpus `expect_domain` was corrected to `multicall`.

### Remaining (not yet done this run)
- **P2 depth:** full Etherscan bulk sweep (≥10k tx, wallet-facing target floors) + Dune cross-chain pinpoint + typed-data corpus (`route_typed_data --require-expect-body`) for COMP `Delegation` + Comet `Authorization`; `expect_body` on all corpus entries; mint/borrow/repayBorrowBehalf/delegateBySig real samples (absent from the recent-2000 window).
- compound-v3 **Bulker** structured decode (grammar extension) + **multichain** Bulker/Rewards (mainnet rewards done).
- **GovernorBravo** voting (castVote/propose) — deferred (no governance-vote ActionBody domain; COMP voting-power delegation IS covered).
- **P3** gap triage, **P4** land (`cargo test --workspace` + wasm-build + `check-onboarding-evidence --phase all`).

## Blockers

| blocker | source | next action |
|---|---|---|
| | | |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is `done` or has a concrete `blocked` disposition and this passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- compound --phase all
```
