# Across — Onboarding Evidence


## Run Metadata

| field | value |
|---|---|
| protocol | across |
| branch | feat/bridge-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-bridge |
| date | 2026-06-05 |
| main agent | Claude (Opus 4.8, 1M) |
| base commit | feabb369 |

## Scope Classification

Use this section to make the final claim precise. This table is narrative
evidence; the phase tables below are the mandatory gate.

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | Ethereum mainnet (eip155:1). Other-chain SpokePools deferred (multichain = separate framework). |
| completion target | `wallet-facing` (cross-chain deposit surface) |
| **pre-decision** cross-entry volume distribution (tx-share of EACH user-facing entry; which dominates) — measured BEFORE the cover/defer boundary (H1) | SpokePool 30d (Dune 7659328): depositV3 19,658 tx / 9,312 EOA; deposit 4,970 tx / 3,150 EOA; other deposit variants 0 tx/30d; relayer fillRelay 56,087 tx/21 EOA (non-user). Periphery (7659341): depositNative 1,934 + swapAndBridge 1,174 EOA. |
| per-cover-candidate wrapper/router selector child resolution-rate (effective coverage = decoded children / real children; NOT manifest-presence) (H3) | n/a — depositV3/deposit are flat single_emit, not wrappers. |
| covered real-usage coverage-share — **volume-weighted protocol-level**: Σ covered top-level tx / Σ all top-level tx across every user-facing entry (NOT per-contract selector-share) (H2), wrappers counted by child resolution-rate (H3) | depositV3+deposit = 12,462 signing-EOA = ~100% of SpokePool deposit signing; ~80% of Across mainnet deposit-signing incl Periphery (re-verified P2). |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | SpokePoolPeriphery (depositNative 1,934 / swapAndBridge 1,174 / depositWithAuthorization 8 EOA, Dune 7659341); SpokePool deposit variants (each 0 tx/30d, Dune 7659328). |
| direct factory-child calls | not applicable — SpokePool is a singleton proxy. |
| final claim label (MUST NOT over-claim the measured coverage-share above) | "Across SpokePool direct deposit, mainnet, ~100% of SpokePool deposit-signing EOA (~80% incl Periphery); deferred = SpokePoolPeriphery, 0-usage deposit variants, multichain." |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification above: mainnet (eip155:1), wallet-facing deposit, multichain deferred. |
| pre-decision cross-entry volume distribution measured BEFORE the cover/defer boundary (tx-share of each user-facing entry; which entry dominates), so cover/defer is data-driven not assumed (H1) | done | Dune 7659328 (SpokePool per-selector) + 7659341 (Periphery), measured BEFORE boundary: depositV3 9,312 EOA + deposit 3,150 EOA dominate; variants 0 tx/30d. |
| Codex current-session research executed | done | n/a — single main-session (Claude Opus). Research = Etherscan getsourcecode/getabi + github across-protocol/contracts + Dune. |
| Claude Code or sub-agent research executed | done | 2 Plan sub-agents (edit-site validation vs governance precedent b4c1427a; field-model/decode validation vs amm::Swap/token::Transfer). |
| Claude/sub-agent exact prompt or command recorded | done | Plan prompts: (1) new-domain edit-site completeness; (2) BridgeAction field model + decode mapping. curl getsourcecode/getabi (SpokePool/impl/Periphery), Dune 7659328/7659341, keccak/cast sig. |
| Codex-only candidates listed | done | n/a — no Codex parallel run. |
| Claude/sub-agent-only candidates listed | done | Agent1: MISSED view.rs (compile-forced) + schema/mod.rs ACTION_CONTEXT_TYPES. Agent2: bare-Send naming, $match/$cases chainId value-map, bytes32 normalization. |
| dropped-unverified candidates listed with reason | done | n/a — all addresses/ABIs 1st-party verified (Etherscan getsourcecode + github V3SpokePoolInterface). |
| final contract inventory verified against first-party sources | done | docs.across.to mainnet (SpokePool 0x5c7b…35C5, SpokePoolPeriphery 0x10D8…B610, MulticallHandler 0x0F7A…3a0E) + Etherscan getsourcecode (impl 0x5e5b…dd3b) + github across-protocol V3SpokePoolInterface.sol. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | n/a — SpokePool is a singleton proxy, not factory/pool-heavy. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | n/a — no factory/pool universe. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | n/a — no factory children. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | n/a — singleton; concrete per-(chain,address,selector) manifests. |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | n/a — no factory children. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | n/a — not pool/factory/vault-heavy. |
| token-surface inventory completed or explicitly scoped out | done | scoped out: Across moves canonical tokens (USDC/WETH/…) and mints nothing on source; inputToken/outputToken reuse existing registryV2/tokens base entries → no new token registration. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | registryV2/surface/across/_deployments.json — 4 contracts (SpokePool cover; impl/Periphery/MulticallHandler exclude). |
| `npm run check:surface` output recorded | done | EXIT=1: [I0] across 4 deployed·1 cover·3 exclude ✓; SpokePool 33 surface·2 cover·31 exclude ✓ (I1/I1'/I3); only fail = I2 cover 0x7b939232/0xad5425c6 NO manifest — expected pre-P1. |

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
| Etherscan MCP/API availability checked | pending | |
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
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | pending | |
| Dune MCP/API availability checked | pending | |
| Dune usage baseline recorded | pending | |
| Dune calibration/query executed with partition WHERE or explicitly blocked | pending | |
| Dune `executionCostCredits` / usage delta recorded | pending | |
| Dune rows returned / selected tx hashes recorded | pending | |
| representative real-tx corpus/golden entries committed or justified | pending | |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | pending | |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered set decodes), **volume-weighted protocol-level (Σ covered top-level tx / Σ all top-level tx across every user-facing entry, NOT per-contract selector-share) (H2)** and **every wrapper/router selector counted by child resolution-rate, not manifest-presence (H3)**, with each user-facing DEFER's usage-share recorded; completion label must not over-claim it | pending | |

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

## Blockers

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| | | |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is `done` or has a concrete, user-visible `blocked` disposition and this command passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <protocol> --phase all
```
