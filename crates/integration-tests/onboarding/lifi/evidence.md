# Protocol Onboarding Evidence — Li.Fi (LiFiDiamond)

> Onboarding evidence ledger for Li.Fi. `check-onboarding-evidence` parses this and cross-checks every mandatory row.

## Run Metadata

| field | value |
|---|---|
| protocol | lifi |
| branch | feat/bridge-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-bridge |
| date | 2026-06-06 |
| main agent | Claude Opus 4.8 (1M context) |
| base commit | c91bcc62 (on top of the Across bridge-domain work) |

## Scope Classification

Use this section to make the final claim precise. This table is narrative
evidence; the phase tables below are the mandatory gate.

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | Ethereum mainnet (chain 1). Other-chain LiFiDiamonds deferred (separate framework). |
| completion target | `wallet-facing` (LiFiDiamond bridge + swap entry surface) |
| **pre-decision** cross-entry volume distribution (tx-share of EACH user-facing entry; which dominates) — measured BEFORE the cover/defer boundary (H1) | Dune q7665132 (30d, 105,023 successful top-level tx, 52 selectors): swap-only (GenericSwapFacet/V3) **49.0%**, swap+bridge (swapAndStartBridge…) **38.1%**, bridge-only (startBridge…) **12.9%**. Cumulative: top-12=86.1%, top-15=92.1%, top-20=96.0%, top-30=99.1%. Cover boundary set AFTER this: cover every selector with ≥1 tx/30d. |
| per-cover-candidate wrapper/router selector child resolution-rate (effective coverage = decoded children / real children; NOT manifest-presence) (H3) | N/A in the multicall_recurse/child-callkey sense. Li.Fi entries are top-level (user signs to the diamond). `swapAndStartBridge` decodes to a Multicall built IN-PLACE from the function's own `SwapData[]`+`BridgeData` (`composite_emit`, no per-child re-routing to other callkeys), so effective coverage = the in-place decode itself, not a child-resolution-rate. |
| covered real-usage coverage-share — **volume-weighted protocol-level**: Σ covered top-level tx / Σ all top-level tx across every user-facing entry (NOT per-contract selector-share) (H2), wrappers counted by child resolution-rate (H3) | Cover set = all 51 selectors with ≥1 tx in 30d = **100.0% of 30d top-level function-tx** (105,012 / 105,012; the remaining 11 tx are bare ETH transfers, empty selector). Li.Fi IS top-level (no internal-trace split needed, unlike Across). Count-weighted per H2. Final P2 re-measure pending. |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | 48 bridge/swap selectors, **each 0 tx/30d** (Dune q7665132): AcrossV3 (start+swap), Hop* L1/L2 ERC20/Native, Optimism, Gnosis, DeBridgeDln, ThorSwap, Relay, Unit-swap, AcrossV4Swap, and `*Packed`/`*Min` calldata-packed variants. Measured zero in window; in coverage.json as `exclude` with `DEFERRED (…0 tx/30d…)` reason. |
| direct factory-child calls | not applicable (not a factory/pool protocol; single diamond entry, 49 delegatecall facets behind one address) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | "Li.Fi LiFiDiamond, Ethereum mainnet — ~100% of 30d top-level function-tx (count-weighted, Dune q7665132) routed to a bridge::send / amm::Swap / composite_emit decoder. Bridge-leg recipient/dstChain/compose + source-swap legs decoded. V1-limited / deferred: non-EVM destinations (Mayan/NEAR/Chainflip nonEVMReceiver), facet-specific dst_token/output/exclusiveRelayer enrichment, `*Packed` variants, multichain." |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Ethereum mainnet (chain 1) only; wallet-facing LiFiDiamond bridge+swap surface; multichain deferred. See Scope Classification. |
| pre-decision cross-entry volume distribution measured BEFORE the cover/defer boundary (tx-share of each user-facing entry; which entry dominates), so cover/defer is data-driven not assumed (H1) | done | Dune q7665132 (30d): swap-only 49.0% / swap+bridge 38.1% / bridge-only 12.9%; 52 active selectors ranked. Etherscan txlist (10k/70.7h) snapshot agrees. Cover boundary = every selector with ≥1 tx/30d (decided after measuring). |
| Codex current-session research executed | done | N/A — single Claude session (no Codex). This session's research = main-session Etherscan/Dune measurement + a general-purpose sub-agent (github lifinance/contracts facet/struct enumeration) + an Explore sub-agent (internal amm/strategy code). |
| Claude Code or sub-agent research executed | done | general-purpose agent: full facet inventory + BridgeData/SwapData/facet-data structs from github lifinance/contracts (commit 5164326c). Explore agent: amm::Swap struct, emit.strategy grammar, builder internals, $fn whitelist. |
| Claude/sub-agent exact prompt or command recorded | done | Agent 1 prompt: "research Li.Fi on-chain surface… BridgeData/SwapData structs + every bridge facet's start/swapAndStart signatures + 4-byte selectors, 1st-party github lifinance/contracts only". Agent 2 (Explore): "amm::Swap struct + emit.strategy grammar + can a strategy build heterogeneous Multicall from one call's params + $fn whitelist". |
| Codex-only candidates listed | done | N/A (no Codex session). |
| Claude/sub-agent-only candidates listed | done | Sub-agent surfaced 51 active + 48 inactive bridge/swap fns + struct layouts. All treated as candidate-only and 1st-party re-verified (below). |
| dropped-unverified candidates listed with reason | done | 0 dropped from the cover set — every one of the 51 covered selectors was `cast sig`-verified to equal its on-chain observed selector (`/tmp/lifi_verify.py`: 51/51 match, 0 unmatched observed). The agent's facet-data struct candidates ALL produced the correct selector (so all verified, none dropped). |
| final contract inventory verified against first-party sources | done | DiamondLoupe `facets()` on-chain eth_call (publicnode) → 49 facets / 203 registered selectors; each facet's ABI fetched via Etherscan getabi (49/49 verified, 0 unverified); merged → 128 mutating / 75 view. 51 covered fns all registered on-chain (51/51). |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | N/A — not pool/factory. Single diamond address (0x1231deb6…); the 49 facets are delegatecall implementations behind it (loupe-enumerated), not a user-callable child-address universe. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | N/A — not pool/factory (see above). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | N/A — not pool/factory. Function-level disposition (128 mutating selectors, cover/exclude) is in lifi-diamond.coverage.json. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | N/A — not pool/factory. Strategy = concrete per-selector manifests (51), template-generated (one BridgeData/SwapData emit body, full ABI per selector). |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | N/A — not a factory protocol; single diamond entry. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | N/A — not pool/factory/vault-heavy. |
| token-surface inventory completed or explicitly scoped out | done | Li.Fi moves canonical tokens and mints none → no new registryV2/tokens needed for decode. amountNano caps use the SW's on-demand token-client (`/tokens/<chain>/<addr>`) over the existing base token set. Any covered-tx token missing decimals surfaces in P2 (nano omitted, cap dormant — fail-safe); will register if observed. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | registryV2/surface/lifi/_deployments.json (LiFiDiamond cover; 49 facets = delegatecall impls behind it, snapshot-gated; LiFiDEXAggregator periphery noted deferred). |
| `npm run check:surface` output recorded | done | `✓ LiFiDiamond [1]: 128 surface · 51 cover · 77 exclude · 0 on-chain manifests`; `✓ [I0] lifi: 1 deployed · 1 cover`. I0+I1 PASS. Remaining ✗ = I2 (51 cover selectors have no manifest yet) — expected at P0, resolved in P1. |

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
| (none so far) | | |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is `done` or has a concrete, user-visible `blocked` disposition and this command passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- lifi --phase all
```
