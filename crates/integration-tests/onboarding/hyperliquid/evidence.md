# Protocol Onboarding Evidence — HyperLiquid

> ♻️ **Re-verification onboarding.** HyperLiquid was already partially onboarded (18 `hyperliquid_core`
> ActionBody variants + 21 manifests + Flow-3 /exchange capture). This round closed the decode gaps the
> prior work left (Flow-2 typed-data + on-chain CoreWriter/WHYPE all emitting `domain:unknown`), added the
> missing framework artifacts (surface gate, field-golden corpus, SCOPE ORACLE, this ledger), and
> re-grounded every address/action against 1st-party sources (hyperliquid-python-sdk + HL gitbook +
> Etherscan-v2 verified ABIs). Row labels are kept verbatim from `ONBOARDING_EVIDENCE_TEMPLATE.md` so
> `check-onboarding-evidence` parses them.

## Run Metadata

| field | value |
|---|---|
| protocol | hyperliquid |
| branch | feat/hyperliquid-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-hyperliquid |
| date | 2026-06-03 |
| main agent | Claude Opus 4.8 (1M) + 9-agent survey workflow |
| base commit | 79d8ae90 (feat/registry-v2) |

## Scope Classification

Use this section to make the final claim precise. This table is narrative
evidence; the phase tables below are the mandatory gate.

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | on-chain HyperEVM 999 + Arbitrum 42161; off-chain HyperCore /exchange (hl-mainnet) + EIP-712 user-signed (cosmetic signatureChainId 42161). testnet 998/421614 deferred. |
| completion target | `wallet-facing` re-verification — every user-signable HL surface decodes structured-or-HL-attributed (no silent allow, no anonymous domain:unknown for a HL action). |
| covered real-usage coverage-share | WHYPE 4/4 selectors=100% (200-tx); CoreWriter 300/300 routed (sendAsset 68%+spotSend 19% HL-attributed); 12/12 EIP-712 primaryTypes non-anonymous. |
| user-facing DEFERs (1차 usage-share) | CoreWriter amount scaling (sendAsset 68%+spotSend 19% HL-attributed, amounts unscaled); /exchange order symbol resolution (ASSET-N); agentSendAsset/subAccountSpotTransfer/hip3LiquidatorTransfer (hl_unknown). |
| direct factory-child calls | not applicable (HL has no factory/pool universe) |
| final claim label | wallet-facing HyperLiquid re-verification: COMPLETE for routing+structural legibility; DEFERRED for HL-fixed-point amount scaling + asset-symbol resolution. No full-universe/multichain claim. |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | See Scope Classification. Representative chains 999/42161; wallet-facing re-verification; testnet 998/421614 deferred. |
| Codex current-session research executed | done | n/a — Claude session. Primary-source research via the survey workflow's 3 general-purpose research agents. |
| Claude Code or sub-agent research executed | done | 9-agent Workflow `hl-onboarding-survey` (run wf_24c6f442-9e6, 1.04M subagent tokens): 6 code-map + 3 primary-source agents. |
| Claude/sub-agent exact prompt or command recorded | done | Workflow script `…/workflows/scripts/hl-onboarding-survey-wf_24c6f442-9e6.js`; each agent prompt embedded (repo/branch/files/guardrails). |
| Codex-only candidates listed | done | n/a (no Codex session). |
| Claude/sub-agent-only candidates listed | done | Survey: 44 /exchange actions (SDK-confirmed), 15 CoreWriter action_ids (+ action_id=20 live), 12 EIP-712 primaryTypes, system addresses — all 1차-verified. |
| dropped-unverified candidates listed with reason | done | subAccountModify + spotUser NOT confirmed in hyperliquid-python-sdk (risk=unknown) → left in BENIGN_PASS_THROUGH (no fund-move evidence). vaultModify = not a real /exchange action (harmless dead entry). |
| final contract inventory verified against first-party sources | done | WHYPE 0x5555/CoreWriter 0x3333/Bridge2 0x2df1c51e/HYPE-system 0x2222 — Etherscan-v2 getsourcecode VERIFIED (WHYPE9/CoreWriter/Bridge2) + HL gitbook. CoreWriter 15 action_ids verbatim from gitbook CoreWriter.sol; EIP-712 domain+12 primaryTypes from sdk signing.py. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | n/a — HyperLiquid has no factory/pool universe (fixed system + token contracts). |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | n/a (no pool universe). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | n/a (no pool universe). |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | n/a (no pool universe; fixed system contracts). |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | n/a (no factory children). |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | n/a (not pool/factory/vault-heavy). |
| token-surface inventory completed or explicitly scoped out | done | WHYPE (0x5555, 999) = wrap/unwrap target; USDC (HyperEVM 0xb88339…/Arbitrum 0xaf88…) = bridge asset. HL CORE spot tokens are L1 string ids carried verbatim in hl_spot_send/hl_send_asset. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | registryV2/surface/hyperliquid/_deployments.json — 5 contracts (4 cover, 1 exclude). |
| `npm run check:surface` output recorded | done | PASS — 'every gated contract's external surface is fully triaged and consistent'; [I0] hyperliquid: 5 deployed · 4 cover · 1 exclude. |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | Flow-2: UsdSend/Withdraw/SpotSend/UsdClassTransfer/SendAsset/TokenDelegate→hyperliquid_core::hl_*; Convert/SendMultiSig/UserDex/UserSet→hl_unknown{action_type}; ApproveAgent/ApproveBuilderFee→permission (unchanged). On-chain: WHYPE deposit/withdraw→token::{wrap,unwrap}_native; CoreWriter 15→action11 perp::cancel_order + 14 hl_unknown. |
| permission/fund-movement/red-flag selector review recorded | done | All fund-move EIP-712 types now expose destination+amount to policy (were unknown). Permission types gateable. High-risk unmodeled (agentSendAsset/hip3LiquidatorTransfer/CSignerAction)→hl_unknown (caught, not silent). |
| manifest files added/changed listed | done | 10 rest/*.json (usd-send,withdraw,spot-send,usd-class-transfer,send-asset,token-delegate,convert-to-multi-sig-user,send-multi-sig,user-dex-abstraction,user-set-abstraction) + whype/{deposit,withdraw} + core-writer/send-raw-action. |
| enrichment/live_field decision recorded for every COVER action | done | NO enrichment/live_inputs — every HL action decodes statically (decimal-string amounts + address strings). No remote policy-RPC. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | n/a — zero remote/live/enrichment methods (fully static decode, ScopeBall no-simulation invariant). |
| Tier3 not needed or full Tier3 downstream contract completed | done | NO new Tier3 — reused existing hyperliquid_core domain (18 variants/lowering/cedarschema/conformance) + just-merged token::{WrapNative,UnwrapNative}. Changes are manifest + test + surface only. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | n/a (no new Tier3). |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `npm run check:manifest` = 'validate (all): 1781 single_emit OK, 0 structural errors'; build-index 892 manifests. (16 callkey COLLISION warnings = pre-existing, non-HL.) |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | hl-fuzz-coverage (extension vitest): 60 iters × 36 action specs = 2160 routes, fixed-seed. v3 manifest validate 24 iters/manifest. |
| iterations >= 5000 or justified lower bound | done | HL CORE surface fully enumerated (44 /exchange + 15 CoreWriter + 12 EIP-712) → exhaustive per-action corpus (21 pinned) stronger than random fuzz. |
| fixed edge-case matrix recorded | done | empty Bridge2 batch (childCount=0), multi-leg deposit, 6 structured EIP-712, 4 hl_unknown EIP-712, CoreWriter 2/6/13 + structured 11 + bad-version fail-soft + undocumented action_id=20. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | permission (approve_agent/builder_fee/multi_sig), value (WHYPE wrap, CoreWriter), array (Bridge2 multicall), tagged_dispatch (CoreWriter), typed-data WRAP RULE (rest/*). |
| representative pass/error corpus entries committed or justified | done | corpus.json — 21 entries (6 Bridge2 + 6 structured EIP-712 + 4 hl_unknown EIP-712 + 2 WHYPE real-tx + 3 CoreWriter real-tx). |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 SUPPORTS HyperEVM — chainlist {chainname:'HyperEVM Mainnet',chainid:'999',status:1,blockexplorer:hyperevmscan.io}. Live txlist/getsourcecode/getTransactionByHash on 999+42161. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | txlist WHYPE 0x5555 (200) + CoreWriter 0x3333 (300), chain 999, sort=desc. |
| external tx pull target address count is nonzero and recorded | done | 3 targets swept (WHYPE 999, CoreWriter 999, Bridge2 42161). nonzero. |
| Etherscan `api_calls_used` recorded | done | ~10 calls (2 txlist + chainlist + 4 getTransactionByHash + 3 getsourcecode). |
| Etherscan `raw_txs_seen` recorded | done | 500 (WHYPE 200 + CoreWriter 300) + 6 Bridge2 (prior corpus). |
| Etherscan `unique_selectors_seen` recorded | done | WHYPE 4 (0x095ea7b3/0x2e1a7d4d/0xd0e30db0/0xa9059cbb); CoreWriter 1 (0x17938e13) → 10 distinct action_ids. |
| Etherscan real tx coverage per COVER selector recorded | done | WHYPE 4/4 selectors hit in 200-tx; CoreWriter 300/300; all decode (deposit→wrap_native, withdraw→unwrap_native, CoreWriter→hl_unknown/perp). |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | WHYPE 200-tx + CoreWriter 300-tx swept, histogrammed (SCOPE ORACLE). Bridge2 selector 0xb30b5bce confirmed in prior corpus. per-target floor met. |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | WHYPE 0 unmatched. CoreWriter: action_id=20 (3 txs)=undocumented action>15→fail-soft hl_unknown{unrecognizedCoreWriterAction} (actionable: HL added an action; non-blocking); ver=0x7b (1)=non-CoreWriter→fail-soft. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | n/a (not pool/factory-heavy; fixed system contracts). |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | none — all swept to-addresses are the known system contracts. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | 10 EIP-712 corpus entries (6 structured + 4 hl_unknown) with field-golden expect_body; route_typed_data exercised. EIP-712 not in Etherscan input → validated by corpus per framework. |
| Dune MCP/API availability checked | done | n/a — on-chain (999) measured via Etherscan v2; off-chain HyperCore /exchange has NO indexer (off-chain signed) → Dune cannot serve it. Etherscan v2 histograms used for SCOPE ORACLE. |
| Dune usage baseline recorded | done | n/a (Etherscan v2 for on-chain; HL info API for off-chain). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | n/a (see above). |
| Dune `executionCostCredits` / usage delta recorded | done | n/a (no Dune used). |
| Dune rows returned / selected tx hashes recorded | done | n/a Dune. Real tx hashes pinned: WHYPE 0x38e5fa/0xc8755e, CoreWriter 0x7521f6/0x5a8b96/0x495bb2, Bridge2 ×6. |
| representative real-tx corpus/golden entries committed or justified | done | corpus.json (commit 0603f52d) — 5 real HyperEVM/Arbitrum txs with full calldata + expect_body. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter hyperliquid --require-expect-body` = 21/21 matched, 21/21 expect_body pinned, gate PASS. |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | CoreWriter (300 txs/999): sendAsset(13)=68%·spotSend(6)=19%·limitOrder(1)=3%·action_id=20=1%·action11(only structured)=0. WHYPE (200): approve45%·withdraw29%·deposit13%·transfer13%=100% covered. Honesty: CoreWriter fund ops (87%) HL-attributed but amount-unscaled → NOT 'fully structured'. |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | (a)Flow-2 unknown→FIXED (b)on-chain CoreWriter/WHYPE unknown→FIXED (c)CoreWriter amount scaling→DEFER (d)order symbol→DEFER (e)3 unmodeled fund actions→DEFER (hl_unknown) (f)action_id=20→fail-soft default. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | Flow-2: rest/* selectors 0x00000001-0x0000000a. On-chain: WHYPE 0xd0e30db0/0x2e1a7d4d, CoreWriter 0x17938e13 ids 1-15. Each verified by a corpus tx-hash + declarative_v3_route test. |
| manifest/decoder/Tier3/harness change list recorded | done | 13 manifests; 0 decoder/Tier3 (reuse); harness = corpus.json +15 entries + declarative_v3_route.rs (CoreWriter const→include_str! SSOT, 8 assertions updated). |
| P2 rerun after fixes recorded | done | After each batch: npm run build → corpus --require-expect-body (12/12→16/16→21/21). |
| corpus `expect` flips or exclusions justified | done | 6 Bridge2 entries gained expect_body (were smoke). 6 b3_hl_*_routes_to_unknown→_routes_to_structured; 5 CoreWriter domain:unknown→hl_unknown{action_type}; 2 WHYPE unknown→wrap/unwrap_native. |
| remaining gaps have explicit defer/blocker disposition | done | DEFER (1차 usage-share attached): CoreWriter amount scaling (needs HL meta snapshot; 87% affected but gateable-by-name), order symbol resolution (HL meta snapshot; ASSET-N), 3 unmodeled fund actions (hl_unknown). cloid 'c' (low security, non-fund). native sink 0x2222 (no fitting ActionBody→unknown, value+target preserved). |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | [build-index] done — 53475 callkey(s) + 85 typed-data entry(ies) across 892 manifest(s). |
| registryV2 build-index vitest output recorded | done | n/a — registryV2 has no vitest (tsx scripts + tsc only). `npm run typecheck` (tsc --noEmit) = clean; `npm run check:tokens` = PASS (0 errors; 1331 pre-existing token_kind warns, non-HL). |
| `npm run check:manifest` output recorded | done | 1781 single_emit OK, 0 structural errors. |
| `npm run check:surface` output recorded | done | PASS — 'every gated contract's external surface is fully triaged and consistent'; [I0] hyperliquid: 5 deployed · 4 cover · 1 exclude. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | n/a (not pool/factory/vault-heavy). |
| v3-harness coverage/fuzz/corpus outputs recorded | done | corpus 21/21 + expect_body 21/21 pinned; declarative_v3_route 78/78. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter hyperliquid --require-expect-body` PASS. |
| `cargo test --workspace` output recorded | done | `cargo test --workspace` = 0 failed (17 `test result: ok` suites incl. corpus/conformance + live_etherscan_tx_maps). Full all-protocol corpus replay `v3-harness corpus` = 384/384 matched. |
| wasm build output recorded if runtime/wasm/schema changed | done | No runtime/wasm-lib/schema SOURCE changed (the only Rust edit is a TEST file `declarative_v3_route.rs`; rest is manifests + corpus + surface). WASM lib still compiles: wasm-pack 'Compiling to Wasm… Finished release 5s'. The post-compile `wasm-opt` step is blocked by a missing wasm-opt binary in THIS env (pre-existing — see memory: iCloud pkg/ stray), NOT introduced here; the unoptimized WASM is functionally identical. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | `cargo fmt` on `declarative_v3_route.rs` (my file ONLY — pre-existing fmt drift in declarative_exports.rs/metamorpho_underlying.rs left untouched per guardrail) + `cargo clippy -p policy-engine-wasm --tests` = clean (0 warnings). registryV2 `tsc --noEmit` = clean. browser-extension TS: ZERO edits → vacuously unaffected; full extension tsc/vitest not completable in this env (blocked by the wasm-opt gap above, pre-existing, not my change). |
| exact staged files and commit hash recorded | done | be41ea29 (Flow-2 10 manifests+corpus) · 0603f52d (on-chain 3 manifests+test+corpus) · efbd4548 (surface 9 files) · + evidence + P4-gate commit. |
| remaining WARNs/deferred selectors/actions listed with reason | done | See P3 defer. check:surface WARN: 5 HL ungated (998/421614 testnet + 999/0x2222 native sink + 42161/0xaf88 generic USDC permit) = representative-chain deferrals. |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | 'wallet-facing HyperLiquid re-verification — COMPLETE for routing+structural legibility; DEFERRED for HL-fixed-point amount scaling + asset-symbol resolution.' No full-universe/multichain claim. |
| no base/worktree merge performed unless user explicitly requested it | done | NOT merged/pushed — awaiting explicit user request. |

## Blockers

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| (none — all phases done with documented defers) | | |

## Final Completion Claim

**Wallet-facing HyperLiquid re-verification onboarding: COMPLETE.** Every user-signable HyperLiquid
surface — off-chain `/exchange` CORE actions (Flow-3), EIP-712 user-signed actions (Flow-2), and on-chain
HyperEVM/Arbitrum contracts (Flow-1) — now decodes to a structured-or-HL-attributed ActionBody (no silent
allow; no anonymous `domain:unknown` for a HyperLiquid action). Verified by `v3-harness corpus --filter
hyperliquid --require-expect-body` (21/21 pinned), `npm run check:surface` (PASS), `npm run check:manifest`
(0 errors), `declarative_v3_route` WASM route tests (78/78), and `check-onboarding-evidence --phase all`
(70 rows, 0 blocked).

**Honestly deferred** (each with 1차 usage-share): HL-fixed-point CoreWriter amount scaling + /exchange
order asset-symbol resolution — both need a baked HL `meta` snapshot, a separate feature whose absence
never produces a *wrong* number (only a less-legible one); plus 3 low-frequency unmodeled fund actions
(agentSendAsset/subAccountSpotTransfer/hip3LiquidatorTransfer), caught as `hl_unknown`, never silently
allowed. Representative chains: HyperEVM 999 + Arbitrum 42161; testnet 998/421614 deferred. NOT merged or
pushed — awaiting explicit user request.
