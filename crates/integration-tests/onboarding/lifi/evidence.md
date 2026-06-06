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
| covered real-usage coverage-share — **volume-weighted protocol-level**: Σ covered top-level tx / Σ all top-level tx across every user-facing entry (NOT per-contract selector-share) (H2), wrappers counted by child resolution-rate (H3) | TWO numbers (don't conflate): **routing = 100.0%** (all 51 selectors with ≥1 tx/30d are covered → every diamond tx routes to a Li.Fi decoder, not blind-warn). **effective FULL-decode = 77.3%** of the 10k window (swap-only 4,031 fully + bridge-leg EVM 3,704); the other **22.6% warn-close**: non-EVM destinations (Solana 1,505=15% + Bitcoin 526 + Sui-class + long tail) where the `dstChainId` value-map is EVM-only (eip155) and the real recipient is a facet bytes32 not yet extracted. Li.Fi IS top-level (no internal-trace split). Count-weighted per H2. |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | 48 bridge/swap selectors, **each 0 tx/30d** (Dune q7665132): AcrossV3 (start+swap), Hop* L1/L2 ERC20/Native, Optimism, Gnosis, DeBridgeDln, ThorSwap, Relay, Unit-swap, AcrossV4Swap, and `*Packed`/`*Min` calldata-packed variants. Measured zero in window; in coverage.json as `exclude` with `DEFERRED (…0 tx/30d…)` reason. |
| direct factory-child calls | not applicable (not a factory/pool protocol; single diamond entry, 49 delegatecall facets behind one address) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | "Li.Fi LiFiDiamond, Ethereum mainnet — routing 100% of 30d/10k top-level function-tx → a Li.Fi decoder; **~77% fully decode** (swap + EVM-destination bridge incl. source-swap legs); **~23% warn-close = non-EVM destinations** (Solana ~15% / Bitcoin / Sui — value-map is EVM-only; real recipient is a facet bytes32, deferred). Also deferred: facet-specific dst_token/output/exclusiveRelayer, `*Packed` variants, multichain. NOT full-universe, NOT multichain, NOT non-EVM-recipient." |

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
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | 51 cover → reuse: 7 swap-only (GenericSwapFacet/V3) → `amm::Swap{aggregator_route}` (Single=single_emit, Multiple/Generic=array_emit per leg); 22 `startBridge` → `bridge::send` (from ILiFi.BridgeData param 0); 22 `swapAndStartBridge` → **`composite_emit` Multicall[ amm::Swap per SwapData[] leg, bridge::send ]**. No new ActionBody domain (bridge+amm both pre-existing). |
| permission/fund-movement/red-flag selector review recorded | done | All 51 covered = fund-movement (bridge/swap). NO user permission-grant selector in the covered set; `setApprovalForBridges`/`setApprovalForHopBridges`/`addDex`/`registerBridge` are OWNER-only config → excluded (coverage.json). Red-flags captured: cross-chain mis-send (`dst_recipient`+`dst_chain_id`), destination compose/arbitrary-call (`has_message`=hasDestinationCall), swap-then-bridge real input asset (composite shows SwapData[0] token/amount, not just the bridged intermediate). |
| manifest files added/changed listed | done | 51 new manifests under `registryV2/manifests/lifi/<facet>/<fn>@1.0.0.json` (22 startBridge + 22 swapAndStartBridge + 7 GenericSwap). full verified ABI (getabi) per selector; cast-verified selector match. |
| enrichment/live_field decision recorded for every COVER action | done | amm::Swap requires 4 live fields (route/expected_amount_out/price_impact_bp/gas_estimate) → set to `source:{kind:user_supplied}` = **dormant** (Li.Fi route is opaque `SwapData.callData`; no registered route calc; value=defaults). bridge::send has no required live field. Quantity caps via existing SW `amountNano` token-client (no new enrichment). No new policy-RPC/live method. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | None required — static decode only; enrichment dormant (production ships empty policies). No new remote method introduced. |
| Tier3 not needed or full Tier3 downstream contract completed | done | No new ActionBody **domain**. Additive engine changes: (1) `BridgeVenue::LiFiDiamond` variant (Cedar `BridgeVenue={name:String}` → NO schema/registration change); (2) new **`composite_emit`** emit-strategy in the WASM route (`declarative_route_request_v3_json`), reusing `build_array_emit`+`build_action_body`, flattening into one heterogeneous Multicall; (3) new `$fn` `token_key_or_native_zero` (0x0 ∨ 0xEeee → native, Li.Fi `LibAsset` convention). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | No new-domain edit-sites (no effect/view/sync/cedarschema/schema-registration/conformance change — additive venue + reuse). Engine files changed: `crates/policy-server/asset-model/action/src/bridge/mod.rs` (BridgeVenue), `crates/policy-engine-wasm/src/declarative_exports.rs` (composite_emit arm), `crates/adapters/mappers/src/declarative/builtin_fn.rs` + `fn_whitelist.json` (new $fn + unit test). |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `validate (all): 2076 single_emit manifest(s) OK, 0 structural errors`; build-index `done — 2275 callkey(s) ... across 1064 manifest(s)` (lifi = 51 callkeys). `check:surface ✓ LiFiDiamond [1]: 128 surface · 51 cover · 77 exclude · 51 on-chain manifests`. Affected-crate tests: mappers 62 / policy-engine 352+ / policy-action 420 / 0 failed. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `v3-harness fuzz --iterations 6000 --seed 42 --filter lifi` → total 150,000 · pass 22,835 · soft 127,165 · **HARD_fail 0 · panicked 0** (CLEAN). soft = random 256-bit destinationChainId value-map miss + random-bytes invalid SwapData → graceful warn-close, not hard errors. |
| iterations >= 5000 or justified lower bound | done | 6000 iterations/callkey × 51 lifi callkeys = 150,000 total. |
| fixed edge-case matrix recorded | done | 16-tx real-tx corpus = the hand/edge matrix: native input (zero `sendingAssetId` → native via `token_key_or_native_zero`), composite swap+bridge Multicall, multi-leg `array_emit` GenericSwap, BridgeData-only single-tuple-param facets (Omni/Polygon, flattened args), compose flag (`has_message`), 16 distinct facets across all 4 decode shapes. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | array (SwapData[] `array_emit` + composite), nested-tuple (BridgeData + facet-data + SwapData), value (native 0x0 / 0xEeee / erc20), path (positional tuple `[idx]` + flattened single-tuple field-name). opcode-stream N/A (Li.Fi is per-facet selectors, not a command mask). permission N/A (no user grant selector; owner-only config excluded). deadline N/A (bridge deadlines are facet-internal, not decoded in V1). |
| representative pass/error corpus entries committed or justified | done | `crates/integration-tests/data/golden/v3-decode/lifi/corpus.json` — 17 real txs: 16 `expect:pass` (EVM dest) with 158 `expect_body` pins (independent `cast decode-calldata`) + 1 `expect:error` (AcrossV4 → Solana non-EVM warn-close, pins that no bogus sentinel recipient is emitted). |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 API (key local-only, crates/integration-tests/.env). chainid=1. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | `account&action=txlist&address=0x1231deb6…&offset=10000&sort=desc` → 10,000 txs, window 2026-06-03→06 (70.7h). The diamond is the single cover address. |
| external tx pull target address count is nonzero and recorded | done | 1 target address (LiFiDiamond), 10,000 raw txs (nonzero). |
| Etherscan `api_calls_used` recorded | done | ~52: 1 txlist (10k) + 49 per-facet getabi (surface merge) + 1 diamond getabi + 1 getsourcecode. |
| Etherscan `raw_txs_seen` recorded | done | 10,000. |
| Etherscan `unique_selectors_seen` recorded | done | 52 (51 covered fn-selectors + 1 empty selector = bare ETH transfers). Matches Dune q7665132. |
| Etherscan real tx coverage per COVER selector recorded | done | All 51 cover selectors observed in the 10k window; routing coverage = 10,000/10,000 = 100.00% route to a covered selector (`/tmp/lifi_coverage.py`). |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | Single-target protocol (one diamond address = the wallet-facing entry). Swept 10,000 tx on it; 10,000 matched a covered selector. Not a multi-target router/manager protocol (no separate per-target sweep needed). |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | 0 unmatched txs carrying a known/covered selector (100% routed). 11 empty-selector txs = bare ETH transfers to the diamond (non-actionable). |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | N/A — not pool/factory. Single diamond; 49 facets are delegatecall impls (loupe), not user-callable child addresses. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | N/A — all txs are to=diamond (single address). No unknown to-address bucket. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | N/A — LiFiDiamond bridge/swap entries are on-chain calldata (Flow 1) only; no EIP-712 typed-data surface (P0 confirmed; coverage.json has no `signed_structs`). |
| Dune MCP/API availability checked | done | Dune MCP. |
| Dune usage baseline recorded | done | q7665132 (30d, partition `block_time >= now()-30d`, `success=true`): 105,023 top-level tx, 52 selectors. |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | q7665132 with `block_time >= now() - interval '30' day` partition filter. |
| Dune `executionCostCredits` / usage delta recorded | done | 0.779 credits (q7665132, free engine). |
| Dune rows returned / selected tx hashes recorded | done | 52 rows. Corpus tx hashes (16) selected from the Etherscan txlist (listed in corpus.json `tx_hash`). |
| representative real-tx corpus/golden entries committed or justified | done | 17-tx corpus committed (data/golden/v3-decode/lifi/corpus.json): 16 EVM facets/4 decode shapes (pass) + 1 non-EVM warn-close (error). |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter lifi --require-expect-body` → **17/17 matched**, semantic expect_body **16/16 pinned** (158 assertions) + 1 expect=error (got=error). |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered set decodes), **volume-weighted protocol-level (Σ covered top-level tx / Σ all top-level tx across every user-facing entry, NOT per-contract selector-share) (H2)** and **every wrapper/router selector counted by child resolution-rate, not manifest-presence (H3)**, with each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | **Routing coverage = 100.00%** (10,000/10,000 diamond txs → a covered selector). **Effective FULL-decode = 77.3%** (swap-only 4,031 + bridge-leg EVM 3,704 = 7,735/10,000). **22.6% (2,265) warn-close = non-EVM destinations** — value-map is EVM-only (every value eip155:*): Solana 1,505 (15%), Bitcoin 20000000000001 = 526, Sui-class 9270000000000000 + long tail. For these Li.Fi sets `BridgeData.receiver = 0x11f1..f1 SENTINEL` and the real recipient is a facet bytes32 (e.g. AcrossV4Data.receiverAddress) → emitting the sentinel would be a WRONG #1-phishing field, so we warn-close (verified by decoding tx 0x011f…: BridgeData.receiver=0x11f1..f1 vs AcrossV4Data.receiverAddress=0x0f64…0d7f). Bridge-leg EVM-decode = 3,704/5,969 = 62.1%. **(H3)** N/A — composite Multicall is in-place, not child re-routing. **DEFER usage-share:** non-EVM dst 22.6% (Solana 15% / Bitcoin 5.3% / …); 48 inactive selectors 0 tx/30d. ⚠️ an earlier draft mapped Solana in the value-map → would have decoded with the bogus sentinel recipient; corrected to EVM-only warn-close (P3 G4). Li.Fi is itself top-level. |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | Buckets: **(G1)** BridgeData-only single-tuple-param facets (Omni 0x782621d8, Polygon 0xaf62c7d6) hard-faulted — decoder flattens a single tuple param to top-level args. **(G2)** bridge `destinationChainId` value-map misses on legit-EVM chains (Ronin/Cronos/Rootstock/Monad/Plume). **(G3)** non-EVM destinations (Solana/Bitcoin/Sui) — BridgeData.receiver is the `0x11f1..f1` non-EVM SENTINEL, real recipient is a facet bytes32. **(G4 — misdecode caught during honest interrogation)** an interim value-map mapped Solana (1151111081099710) → the decode SUCCEEDED but emitted the sentinel as `dst_recipient` = a WRONG #1-phishing field (verified: tx 0x011f… BridgeData.receiver=0x11f1..f1 vs AcrossV4Data.receiverAddress=0x0f64…0d7f). 0 `unknown_protocol_address`; 0 hard fuzz failures. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | **G1** → 0x782621d8 / 0xaf62c7d6 (fuzz seed 0x958642a3635015db) → flattened field-name args when `len(inputs)==1`. **G2** → +5 EVM dest chains (2020/25/30/143/98866) to the value-map. **G4** → value-map made **EVM-only** (removed Solana 1151111081099710 + Tron 728126428); all non-EVM destinations now `ValueMapNoMatch → warn-close` (no bogus recipient). Pinned by corpus tx 0x011f… `expect:error`. |
| manifest/decoder/Tier3/harness change list recorded | done | decoder: `composite_emit` strategy (declarative_exports.rs), `token_key_or_native_zero` $fn (builtin_fn.rs). manifests: 51 regenerated (G1 single-tuple-flatten + G2 EVM-chain adds + G4 EVM-only value-map). corpus: +1 non-EVM warn-close pin. No harness/Tier3/Rust change for G4 (value-map is manifest data). |
| P2 rerun after fixes recorded | done | post-fix: fuzz 150,000 → HARD_fail 0/panicked 0; corpus **17/17** (16 pass + 158 pins + 1 error); routing 100%; **effective full-decode 77.3%** (honest EVM-only; the interim 92.4% counted Solana with the bogus sentinel recipient and was an overclaim — corrected). |
| corpus `expect` flips or exclusions justified | done | The AcrossV4→Solana entry (0x011f…) is `expect:error` (not pass) — deliberately pins the non-EVM warn-close so a future regression that re-emits the sentinel recipient fails the gate. 16 EVM entries `expect:pass`. |
| remaining gaps have explicit defer/blocker disposition | done | **Deferred (measured):** non-EVM destinations (G3/G4, **22.6% of 10k** — Solana ~15% / Bitcoin ~5.3% / Sui) warn-close; correct decode needs per-facet `BridgeRecipient::Raw` from the facet bytes32 (AcrossV4Data.receiverAddress / MayanData.nonEVMReceiver) + non-eip155 CAIP-2 — the highest-value next enhancement (Solana is ~25% of Li.Fi→Across). Also deferred: 48 inactive selectors (0 tx/30d); facet `dst_token`/`output_amount`/`exclusiveRelayer`; `*Packed` variants. Source-swap leg IS decoded (composite_emit). No blockers. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | `[build-index] done — 53856 callkey(s) + 88 typed-data entry(ies) written across 1064 manifest(s)` (lifi = 51 callkeys). |
| registryV2 build-index vitest output recorded | done | N/A — registryV2 has no `test`/vitest script (`npm run` = build, check:universe, check:surface, check:tokens, check:manifest, check:manifest:full, typecheck, serve). Gate = build + check:* + typecheck. `npm run typecheck` (tsc --noEmit) clean. `npm run check:tokens` PASS (0 errors, 1338 pre-existing warns). |
| `npm run check:manifest` output recorded | done | `validate (all): 2076 single_emit manifest(s) OK, 0 structural errors`. |
| `npm run check:surface` output recorded | done | `PASS — every gated contract's external surface is fully triaged and consistent.` ✓ LiFiDiamond [1]: 128 surface · 51 cover · 77 exclude · 51 on-chain manifests; ✓ I0 lifi 1 cover. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | N/A — not pool/factory/vault-heavy (single diamond entry). |
| v3-harness coverage/fuzz/corpus outputs recorded | done | coverage: `callkeys=2275 ... unique_bundles=1064 install_failures=0` (composite_emit bundles install clean). fuzz: 150,000 / HARD_fail 0 / panicked 0. corpus: 17/17 matched. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter lifi --require-expect-body` → **17/17 matched** · semantic expect_body 16/16 pinned (158 assertions) + 1 expect=error (non-EVM warn-close). |
| `cargo test --workspace` output recorded | done | `passed=1476 failed=0` (exit 0). |
| wasm build output recorded if runtime/wasm/schema changed | done | `./scripts/wasm-build.sh` → `✨ Done`, wasm copied to browser-extension/backend/wasm/ + public/wasm/. `.d.ts` includes `lifi_diamond`. (runtime change: composite_emit strategy + BridgeVenue variant.) |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | `cargo clippy -p policy-action -p policy-engine-wasm -p mappers --all-targets` → Finished, 0 warnings. `cargo fmt -p … -- --check` exit 0. Extension `tsc --noEmit` exit 0; extension vitest 586 passed / 1 skipped / 0 failed (58 files). |
| exact staged files and commit hash recorded | done | P0 `459bcbb1` (surface 3 files + evidence). P1 `712ab430` (bridge/mod.rs + declarative_exports.rs + builtin_fn.rs + fn_whitelist.json + 51 manifests + evidence). P2+P3 `d111a08b` (manifests map/single-tuple fix + corpus.json + evidence). P4 = this evidence commit (see `git log feat/bridge-onboarding`). NO `git add -A` used. |
| remaining WARNs/deferred selectors/actions listed with reason | done | **Deferred:** non-EVM destinations (**22.6% of 10k** — Solana ~15% / Bitcoin / Sui — warn-close; need per-facet `BridgeRecipient::Raw` from facet bytes32 + non-eip155 CAIP-2); 48 inactive bridge/swap selectors (0 tx/30d); facet-specific dst_token/output_amount/exclusiveRelayer enrichment; `*Packed` calldata variants; LiFiDEXAggregator periphery; multichain. **Pre-existing WARNs (not lifi):** check:tokens 1338 warns/0 errors; build-index "skipped 239 sourced duplicate callkey"; check:surface I0' aave/morpho stale-deployment notes. |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | "Li.Fi LiFiDiamond, Ethereum mainnet — routing 100% of 30d/10k top-level function-tx → a Li.Fi decoder; **~77% fully decode** (swap + EVM-destination bridge incl. source-swap legs); **~23% warn-close = non-EVM destinations** (Solana ~15% / Bitcoin / Sui — value-map EVM-only, real recipient = facet bytes32 = deferred). Facet enrichment + `*Packed` + multichain + non-EVM-recipient deferred. NOT full-universe / NOT multichain / NOT non-EVM-recipient." |
| no base/worktree merge performed unless user explicitly requested it | done | No merge/push. 4 commits on `feat/bridge-onboarding` (local only), on top of the Across work. Awaiting explicit user request to push/merge. |

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
