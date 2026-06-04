# Protocol Onboarding Evidence Template

> Copy this file to `crates/integration-tests/onboarding/<protocol>/evidence.md` for each protocol onboarding run.
> This is a completion gate, not a nice-to-have note. If any mandatory row is missing, the phase is incomplete.

## Run Metadata

| field | value |
|---|---|
| protocol | seaport |
| branch | feat/seaport-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-seaport |
| date | 2026-06-04 |
| main agent | Claude Opus 4.8 (1M) |
| base commit | feabb369 (feat/registry-v2) |

## Scope Classification

Use this section to make the final claim precise. This table is narrative
evidence; the phase tables below are the mandatory gate.

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | ethereum mainnet (eip155:1) only. Other chains (Seaport is multichain) = deferred. |
| completion target | `wallet-facing` (Seaport 1.6 core: off-chain order sign + on-chain fulfill/match/cancel) |
| **pre-decision** cross-entry volume distribution (tx-share of EACH user-facing entry; which dominates) — measured BEFORE the cover/defer boundary (H1) | Dune query 7651765 (ethereum.transactions, Seaport 1.6, 14d direct-tx): fulfillAdvancedOrder 82,614 (15,591 senders) · fulfillAvailableAdvancedOrders 59,471 · cancel 45,770 · fulfillBasicOrder_efficient(0x00000000) 34,290 · matchAdvancedOrders 15,508 · fulfillBasicOrder 463 · incrementCounter 345 · fulfillOrder 171 · validate 49 · fulfillAvailableOrders 3. Seaport 1.5 ~73 tx, 1.4 = 0. → cover the advanced + basic-efficient + cancel/incrementCounter; defer 1.5/1.4; exclude validate (49 tx, benign). |
| per-cover-candidate wrapper/router selector child resolution-rate (effective coverage = decoded children / real children; NOT manifest-presence) (H3) | N/A — Seaport fulfill/match selectors are single-action settlement entrypoints, not wrappers (no multicall_recurse/opcode_stream child dispatch). Each covered selector decodes to one FulfillOrder/CancelOrder/SignOrder body; child-resolution-rate ≈ self. (Batch fulfill aggregates multiple orders into one coarse body — documented, not a wrapper.) |
| covered real-usage coverage-share — **volume-weighted protocol-level**: Σ covered top-level tx / Σ all top-level tx across every user-facing entry (NOT per-contract selector-share) (H2), wrappers counted by child resolution-rate (H3) | measured in P2 (see P2 SCOPE ORACLE row) |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | Seaport 1.5 ~73 tx/14d (~0.03% of 1.6 direct); Seaport 1.4 0 tx/14d; SeaDrop primary-mint (separate surface, not measured this round). validate 49 tx/14d (~0.02%, excluded). |
| direct factory-child calls | not applicable (Seaport is a singleton settlement contract, not factory/pool-heavy) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | "Seaport 1.6 wallet-facing onboarding, ethereum mainnet — off-chain OrderComponents signature + 10 on-chain fulfill/match/cancel selectors; Seaport 1.5/1.4 + SeaDrop + multichain deferred; validate excluded." (final share in P2/P4) |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | mainnet only; wallet-facing Seaport 1.6 core; multichain deferred. See Scope Classification. |
| pre-decision cross-entry volume distribution measured BEFORE the cover/defer boundary (tx-share of each user-facing entry; which entry dominates), so cover/defer is data-driven not assumed (H1) | done | Dune query 7651765 (see Scope Classification H1 row). Advanced fulfill variants dominate; 1.5/1.4 negligible→defer; validate negligible→exclude. |
| Codex current-session research executed | done | main session (Claude Opus 4.8): Etherscan getabi/getsourcecode + cast selectors; saved logs/seaport/{seaport16.abi.json,P0_RESEARCH.md}. |
| Claude Code or sub-agent research executed | done | Workflow seaport-p0-research (3 parallel general-purpose agents): eip712 type-graph (1st-party ProjectOpenSea/seaport-types v1.6.3 + on-chain domainSeparator recompute), address triage (Etherscan getsourcecode ×6), on-chain calldata shapes (verified ABI). |
| Claude/sub-agent exact prompt or command recorded | done | Workflow run wf_d7e52c09-83e (script seaport-p0-research-*.js); prompts embedded per agent (eip712-types / address-triage / onchain-calldata). |
| Codex-only candidates listed | done | none unique — main-session surface (12 external-mutating fns via cast) fully overlapped sub-agent calldata enumeration. |
| Claude/sub-agent-only candidates listed | done | EIP-712 ORDER_TYPEHASH 0xfa4456…2c2f + OfferItem/ConsiderationItem type graph + domain name="Seaport" gotcha (vs name()="Consideration") surfaced by eip712 agent. |
| dropped-unverified candidates listed with reason | done | none dropped; all surface from verified Etherscan ABI + 1st-party GitHub. |
| final contract inventory verified against first-party sources | done | Seaport 1.6/1.5/1.4 + Conduit + ConduitController + SeaDrop all Etherscan getsourcecode verified non-proxy (names confirmed). EIP-712 verified by on-chain domainSeparator 0xfce34bc6…ba64 recomputation. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | not applicable — Seaport is a singleton settlement contract (no factory/pool universe). |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | not applicable (singleton). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | not applicable (singleton). |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | not applicable (singleton); per-selector concrete manifests. |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | not applicable (singleton). |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable (no _address_universe.json; Seaport is not pool/factory-heavy). |
| token-surface inventory completed or explicitly scoped out | done | scoped out — NFT collections are unbounded; no individual token registration. setApprovalForAll(operator=Conduit) covered by standard/erc721 (tokens:erc721 auto-enumerate). No protocol-issued token. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | registryV2/surface/seaport/_deployments.json (6 contracts: 1 cover, 5 exclude incl. 3 measured-defers). |
| `npm run check:surface` output recorded | done | I0+I1 PASS for seaport (Seaport16: 12 surface · 10 cover · 2 exclude). Remaining ✗ = I2 cover-selector-without-manifest + S2 OrderComponents-without-manifest = expected pre-P1 (manifests authored in P1; re-run green at P4). /tmp/seaport-p0/checksurface.txt |

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
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- seaport --phase all
```
