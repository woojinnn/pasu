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
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | depositV3 (0x7b939232) + deposit (0xad5425c6) → bridge::send (new domain). = entire user deposit surface (Dune 7659328). |
| permission/fund-movement/red-flag selector review recorded | done | both COVER = fund-movement (outbound bridge). Red-flag signals in BridgeSendAction: dstRecipient (cross-chain misdirection=irreversible), dstChainId, dstToken vs srcToken, hasMessage (compose/dest-exec). No permission-grant selectors in scope (speedUp* deferred, 0 tx). |
| manifest files added/changed listed | done | registryV2/manifests/across/spoke-pool/deposit-v3@1.0.0.json + deposit@1.0.0.json (single_emit → bridge::send). |
| enrichment/live_field decision recorded for every COVER action | done | no live_field/enrichment — all fields static from calldata. *Nano/*Usd = host-populated Cedar projections (not manifest-driven). dst_chain_id via $match/$cases value-map; bytes32 tokens/recipient via address_from_uint256 $fn; has_message via bytes_nonempty $fn. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | n/a — no remote policy-RPC / live / enrichment methods (static decode only). |
| Tier3 not needed or full Tier3 downstream contract completed | done | Tier3 COMPLETED — new `bridge` domain (axis-1). 12 edit sites + 3-site Cedar registration + conformance; cargo test -p policy-transition -p policy-engine 0 fail (incl send_lowering_conforms). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | ActionBody: action/src/bridge/{mod,send}.rs + lib.rs + view.rs. effect: transition/src/effect/bridge.rs + effect/mod.rs. sync: actions/walk/mod.rs (2 arms). lowering: lowering_v2/bridge/{mod,send}.rs + dispatch.rs + mod.rs. cedar: actions/bridge/send.cedarschema. registration: schema/{mod.rs const+SHIPPED+ACTION_CONTEXT_TYPES, action_name.rs +assert119, per_policy.rs +import+assert121}. apply.rs Reducer arm. $fn: builtin_fn.rs bytes_nonempty + fn_whitelist.json. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | check:manifest: 2051 single_emit OK, 0 structural errors (24 iters/manifest, source-ref representative). check:surface across green (2 cover/2 manifests/I0). mappers 62 tests (whitelist lockstep). policy-transition+policy-engine 0 fail. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | v3-harness fuzz --iterations 5000 --seed 0 --filter across |
| iterations >= 5000 or justified lower bound | done | 5000/callkey × 2 callkeys = 10000 total. pass=411 soft=9589 fail=0 panic=0 (soft = random destinationChainId → value-map ValueMapNoMatch fail-loud, by design). |
| fixed edge-case matrix recorded | done | real corpus covers: address-typed (depositV3) vs bytes32-typed (deposit) recipient/tokens; no-message vs with-compose-message (deposit recipient=MulticallHandler); zero exclusiveRelayer (lowering-omitted); 2 dst chains (Base 8453 / Arbitrum 42161); payable. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | bridge has no permission/nested/array/opcode legs. Covered edges: bytes32↔address recipient/token (address_from_uint256), CAIP-2 value-map (matched + unknown→fail-loud, 9589 fuzz soft), has_message bool ($fn bytes_nonempty), zero-relayer omit, payable value. deadline fields decoded but not policy-surfaced (deferred). |
| representative pass/error corpus entries committed or justified | done | 2 pass entries committed (across/corpus.json). No error entries — all in-scope selectors decode; deferred/excluded selectors are surface-triaged (not corpus). |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 API live (chainid=1): getsourcecode, getabi, txlist. Key from crates/integration-tests/.env (local). |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | txlist address=0x5c7b…35C5 (SpokePool) page=1 offset=200 sort=desc. |
| external tx pull target address count is nonzero and recorded | done | 1 target (SpokePool); 200 txs pulled. |
| Etherscan `api_calls_used` recorded | done | ~5 (getsourcecode ×1, getabi ×2 [impl + periphery], txlist ×1, deposit-decode local). |
| Etherscan `raw_txs_seen` recorded | done | 200 (txlist offset). |
| Etherscan `unique_selectors_seen` recorded | done | per Dune 7659328 (full 30d): 7 selectors on SpokePool — depositV3, deposit, fillRelay, fillV3Relay, multicall, requestSlowFill, (empty/eth). |
| Etherscan real tx coverage per COVER selector recorded | done | depositV3 0x7b939232 → tx 0x5ba1…; deposit 0xad5425c6 → tx 0xde48…; both decode green. |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | SpokePool is the single wallet-facing target; swept via txlist (200) + Dune per-selector (7659328). target file = surface/across/spoke-pool.coverage.json. raw=200, matched cover-selectors = depositV3+deposit. |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | non-deposit selectors (fillRelay 56K/fillV3Relay/requestSlowFill/multicall) = non-actionable: relayer/keeper/batch, surface-EXCLUDED. 0 actionable unmatched. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | n/a — SpokePool singleton, no factory/pool universe. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | n/a — single contract (SpokePool); SpokePoolPeriphery is a separate known contract (deferred), not an unknown. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | n/a — Across deposit is Flow 1 (on-chain calldata), no user EIP-712 in scope. speedUp* (depositorSignature) deferred, 0 tx/30d. |
| Dune MCP/API availability checked | done | Dune MCP live. |
| Dune usage baseline recorded | done | queries 7659328 (SpokePool per-selector), 7659341 (Periphery), 7651935/7652023 (bridge landscape), 7652321 (OFT). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | yes — all queries use WHERE block_time >= CURRENT_DATE - INTERVAL '30' DAY (partition prune). |
| Dune `executionCostCredits` / usage delta recorded | done | 7659328 ≈ 0.62 credits; 7659341 ≈ 0.60; free engine. |
| Dune rows returned / selected tx hashes recorded | done | 7659328 → 7 selector rows; selected corpus tx hashes 0x5ba1578f… (depositV3), 0xde48fbb4… (deposit) via Etherscan txlist. |
| representative real-tx corpus/golden entries committed or justified | done | crates/integration-tests/data/golden/v3-decode/across/corpus.json — 2 real mainnet txs. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | v3-harness corpus --filter across --require-expect-body → 2/2 matched; semantic expect_body 2/2 pass entries pinned (9 + 8 field pins). |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered set decodes), **volume-weighted protocol-level (Σ covered top-level tx / Σ all top-level tx across every user-facing entry, NOT per-contract selector-share) (H2)** and **every wrapper/router selector counted by child resolution-rate, not manifest-presence (H3)**, with each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | H2: depositV3+deposit = 12,462 signing-EOA = ~100% of SpokePool deposit-signing (Dune 7659328); ~80% of Across mainnet deposit-signing incl Periphery. H3 n/a (flat single_emit, not wrapper). DEFER usage-share: SpokePoolPeriphery ~3,108 EOA (7659341); SpokePool deposit variants each 0 tx/30d; multicall 6 tx. Completion label does not over-claim (SpokePool deposit only). |

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
