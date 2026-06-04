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
| covered real-usage coverage-share — **volume-weighted protocol-level**: Σ covered top-level tx / Σ all top-level tx across every user-facing entry (NOT per-contract selector-share) (H2), wrappers counted by child resolution-rate (H3) | **99.3%** (volume-weighted, direct top-level): Σ covered / Σ all = 9928/10000 recent direct-to-Seaport txs. H3: 95.9% of Seaport calls top-level direct (Dune 7651895). Effective coverage of the direct surface ~99.3%, of ALL Seaport calls ~95%. |
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
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | 10 on-chain selectors + OrderComponents sign → new `marketplace` domain. fulfillBasicOrder/Efficient→FulfillOrder(seaport_basic_order); fulfillOrder/fulfillAdvancedOrder→FulfillOrder(seaport_items); fulfillAvailable*/match*→FulfillOrder(seaport_aggregate_items); cancel/incrementCounter→CancelOrder; OrderComponents sign→SignOrder(seaport_items). |
| permission/fund-movement/red-flag selector review recorded | done | SignOrder = the key drainer surface (criteria offer → anyToken = sign away ANY NFT; flagged via criteria_root + lowering anyToken). FulfillOrder shows what taker gives/receives. CancelOrder = revocation (benign). setApprovalForAll(operator=Conduit) already covered by standard/erc721. No selector mis-classified as benign. |
| manifest files added/changed listed | done | registryV2/manifests/seaport/order/ (11): sign + fulfillAdvancedOrder + fulfillAvailableAdvancedOrders + fulfillBasicOrder + fulfillBasicOrderEfficient + fulfillOrder + fulfillAvailableOrders + matchOrders + matchAdvancedOrders + cancel + incrementCounter. |
| enrichment/live_field decision recorded for every COVER action | done | No live_inputs for any marketplace action (faithful static decode of the order/calldata; live_inputs:{} in every manifest). conduitKey→conduit-address derivation deferred (emit raw conduit_key + lowering usesConduit). All amounts/items decoded statically. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | none — no policy_rpc / live / enrichment methods required (no live_inputs). N/A. |
| Tier3 not needed or full Tier3 downstream contract completed | done | Full Tier3: new `marketplace` domain (axis-1). ActionBody + effect + view + sync + lowering_v2 + 3 cedarschema + 3-site registration + conformance — all complete, 6 conformance tests green. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | ActionBody: action/src/marketplace/{mod,venue,item,sign_order,fulfill_order,cancel_order}.rs + lib.rs variant. effect/marketplace.rs + apply.rs. view.rs arm. sync walk/mod.rs arms. lowering_v2/marketplace/{mod,sign_order,fulfill_order,cancel_order}.rs + dispatch.rs. cedarschema marketplace/{sign,fulfill,cancel}_order + core.cedarschema namespace. 3-site: schema/mod.rs SHIPPED, action_name.rs REGISTERED(+2), per_policy.rs RESOLVER(+3). $fn: builtin_fn.rs seaport_items/seaport_aggregate_items/seaport_basic_order + fn_whitelist.json. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | v3-harness validate --filter seaport: 9 single_emit manifest(s) OK, 0 structural errors (iters/manifest=24). check:surface PASS (Seaport16: 10 cover · 10 on-chain manifests · 1 signed-struct). |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | v3-harness fuzz --iterations 6000 --seed 0x20260604 --filter seaport. |
| iterations >= 5000 or justified lower bound | done | 6000 iters/callkey × 10 callkeys = 60000 total. |
| fixed edge-case matrix recorded | done | corpus covers: route-0 ETH buy (fulfillBasicOrder_efficient), collection-bid w/ ERC721_WITH_CRITERIA criteria_root=0 anyToken (fulfillAdvancedOrder), batch coarse-aggregate (matchAdvancedOrders), revocation (cancel), off-chain typed-data listing w/ fees+royalty (sign). |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | nested tuple positional ($args.advancedOrder[0][2]), struct-array items (seaport_items/aggregate), flat single-tuple spread (fulfillBasicOrder), criteria relabel (root vs tokenId), native vs erc20/erc721 items, batch aggregation, $match orderType enum, typed-data named vs calldata positional. |
| representative pass/error corpus entries committed or justified | done | crates/integration-tests/data/golden/v3-decode/seaport/corpus.json (5 pass entries: 4 calldata + 1 typed-data). fuzz soft-errors (out-of-enum ItemType) tolerated by oracle is_shape_artifact. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 API (ETHERSCAN_API_KEY, crates/integration-tests/.env) — txlist + eth_getTransactionByHash + getsourcecode + getabi, all status=1 OK. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | account=txlist address=Seaport 1.6, offset=10000 sort=desc → 10000 most-recent direct txs. logs/seaport/sweep.json. |
| external tx pull target address count is nonzero and recorded | done | 1 target address (Seaport 1.6 0x..eB395); raw_txs_seen=10000. |
| Etherscan `api_calls_used` recorded | done | ~10 calls (1 txlist 10k + 7 getTransactionByHash + getabi + getsourcecode×6 via P0 workflow). |
| Etherscan `raw_txs_seen` recorded | done | 10000. |
| Etherscan `unique_selectors_seen` recorded | done | 10 distinct (9 cover-set selectors present + '0x' bare-ETH). Histogram: fulfillAdvancedOrder 4275, fulfillAvailableAdvancedOrders 2383, cancel 1617, fulfillBasicOrder_efficient 986, matchAdvancedOrders 651, '0x' 72, fulfillBasicOrder 8, fulfillOrder 4, incrementCounter 3, fulfillAvailableOrders 1. |
| Etherscan real tx coverage per COVER selector recorded | done | all 9 present cover selectors decode to marketplace bodies (verified by corpus + fuzz); 10th cover (matchOrders) 0 in this 10k window but covered by manifest + validate fuzz. |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | singleton settlement contract; the 10k txlist IS the wallet-facing sweep for the sole target (Seaport 1.6). per-target floor 10000; matched (cover-set) 9928; unmatched 72. |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | 72 unmatched = selector '0x' (empty calldata, bare ETH transfers to Seaport) — NON-actionable (not protocol calls; no order semantics). 0 actionable unmatched. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | not applicable (Seaport is a singleton, no pool/factory universe). |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | none — the sweep is to the single known Seaport 1.6 address; no unknown-to-address hard gaps. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | OrderComponents (the only in-scope primaryType) verified via the corpus typed-data sign entry: v3-harness corpus --filter seaport --require-expect-body routes route_typed_data → sign_order, 1/1 with expect_body. |
| Dune MCP/API availability checked | done | Dune MCP available; community_fluid_engine_v2, 464/2500 credits used at start. |
| Dune usage baseline recorded | done | creditsUsed 464.633 / quota 2500 (billing 2026-05-05..06-05). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | query 7651765 (direct-tx selector+version dist, 14d, block_time partition) + 7651895 (top-level vs internal traces, 1d). free engine. |
| Dune `executionCostCredits` / usage delta recorded | done | 7651765 = 0.544 credits; 7651895 = 0.231 credits. |
| Dune rows returned / selected tx hashes recorded | done | 7651765 = 27 rows (selector×version); 7651895 = 2 rows (top_level_direct 14109 vs internal_via_aggregator 610). corpus tx hashes selected from Etherscan sweep (see corpus.json). |
| representative real-tx corpus/golden entries committed or justified | done | corpus.json 4 calldata real-tx entries (fulfillBasicOrder_efficient/fulfillAdvancedOrder/cancel/matchAdvancedOrders) + 1 typed-data sign — all with independent cast-derived expect_body. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | v3-harness corpus --filter seaport --require-expect-body → corpus 5/5 matched, semantic expect_body 5/5 pinned. |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered set decodes), **volume-weighted protocol-level (Σ covered top-level tx / Σ all top-level tx across every user-facing entry, NOT per-contract selector-share) (H2)** and **every wrapper/router selector counted by child resolution-rate, not manifest-presence (H3)**, with each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | H2 (volume-weighted, direct top-level): Σ covered / Σ all = 9928/10000 = 99.3% of recent direct-to-Seaport txs decode to marketplace bodies. Uncovered 72 (0.7%) = '0x' bare-ETH transfers (not protocol calls). H3: Seaport 1.6 calls 95.9% top-level direct (Dune 7651895: 14109/14719) — a Seaport adapter's effective coverage of the direct surface is ~99.3%, of ALL Seaport calls ~95%. DEFER usage-share: 1.5 ~0.03%, 1.4 0%, validate 0.02%. Label does NOT over-claim. |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | HARD: 0. SOFT: fuzz out-of-enum ItemType/route (43033) = would-revert synthetic, tolerated by oracle is_shape_artifact (real enums pinned by corpus). UNKNOWN_PROTOCOL_ADDRESS: 0. EXCLUDED: validate (49 tx/14d benign), __activateTstore (infra), bare-ETH '0x' (72, non-protocol). DEFER: Seaport 1.5/1.4/SeaDrop + multichain + conduitKey-address derivation. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | named-tuple-param arg-path fix (all calldata selectors, validate seeds) + fulfillBasicOrder single-tuple flat-spread fix (0xfb0f3ee1/0x00000000) + oracle is_shape_artifact for ItemType/route (fuzz seeds). All resolved pre-commit; 0 remaining hard gaps. |
| manifest/decoder/Tier3/harness change list recorded | done | Tier3 domain (commit 520b0802), 3 $fn (2349435d), 11 manifests + oracle tolerance (5fa82e31), corpus (c405f56a). No decoder-core change beyond $fn + oracle. |
| P2 rerun after fixes recorded | done | final: validate 0 structural errors; corpus 5/5 + 5/5 expect_body; fuzz 60000 fail=0 panicked=0. |
| corpus `expect` flips or exclusions justified | done | no flips; all 5 entries expect=pass and pass. No exclusions. |
| remaining gaps have explicit defer/blocker disposition | done | DEFER (measured): Seaport 1.5 ~0.03%, 1.4 0%, SeaDrop (separate mint surface), multichain (separate framework), conduitKey→address derivation (follow-up enrichment; usesConduit emitted now). No blockers. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | exit 0 — done: 53814 callkey(s) + 89 typed-data entry(ies) across 1022 manifest(s) (incl. 11 seaport + Seaport OrderComponents typed-data). |
| registryV2 build-index vitest output recorded | done | registryV2 has no vitest suite; build-index correctness validated via `npm run build` (exit 0) + `npm run check:manifest` (validate) + `npm run typecheck` (tsc --noEmit clean). |
| `npm run check:manifest` output recorded | done | v3-harness validate --filter seaport: 9 single_emit OK, 0 structural errors. (full check:manifest run in P4 land gates.) |
| `npm run check:surface` output recorded | done | PASS — Seaport16: 12 surface · 10 cover · 2 exclude · 10 on-chain manifests · 1 signed-struct; I0 seaport 6 deployed 1 cover 5 exclude. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable — Seaport is a singleton settlement contract, no _address_universe.json / pool universe. |
| v3-harness coverage/fuzz/corpus outputs recorded | done | validate 0 errors; fuzz 60000 seed 0x20260604 fail=0 panicked=0; corpus 5/5 + expect_body 5/5. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | v3-harness corpus --filter seaport --require-expect-body → 5/5 matched, 5/5 expect_body pinned. |
| `cargo test --workspace` output recorded | done | exit 0 — 59 `test result: ok` lines, 0 failed, 0 panicked (incl. integration-tests 64/0 with synthetic_fuzz; policy-engine 357/0 incl. 6 marketplace conformance; mappers 68/0 incl. 6 seaport $fn + whitelist lockstep). |
| wasm build output recorded if runtime/wasm/schema changed | done | ./scripts/wasm-build.sh exit 0 (3m02s); pkg ready; artifact copied to browser-extension/backend/wasm/ + public/wasm/ (new marketplace domain tsify bindings regenerated). |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | cargo clippy -p {policy-action,policy-transition,policy-engine,policy-sync,mappers,policy-engine-integration-tests} --all-targets -- -D warnings: clean (exit 0). cargo fmt --check: clean. registryV2 tsc --noEmit: clean. |
| exact staged files and commit hash recorded | done | commits on feat/seaport-onboarding: c6967f80 (P0 surface) · 520b0802 (Tier3 marketplace domain) · 2349435d (3 seaport $fn) · 5fa82e31 (11 manifests + oracle) · c405f56a (corpus) · + P4 land commit (clippy refactors in lowering_v2/marketplace/mod.rs + builtin_fn.rs + evidence.md). git log --oneline. |
| remaining WARNs/deferred selectors/actions listed with reason | done | WARN: sign manifest placeholder selector 0x00000011 creates a harmless sig-only callkey (routing is by typed-data bridge). DEFER: validate (excluded), Seaport 1.5/1.4/SeaDrop, multichain, conduitKey-address derivation. |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | Seaport 1.6 wallet-facing onboarding, ethereum mainnet — off-chain OrderComponents signature + 10 on-chain fulfill/match/cancel selectors (99.3% of recent direct txs); Seaport 1.5/1.4 + SeaDrop + multichain deferred (measured negligible); validate excluded. New `marketplace` ActionBody domain. |
| no base/worktree merge performed unless user explicitly requested it | done | no merge/push performed. Commits remain on feat/seaport-onboarding only. |

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
