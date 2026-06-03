# 1inch — Onboarding Evidence Ledger

> Greenfield run of the ScopeBall protocol-onboarding framework (`ONBOARDING_PROMPT.md`,
> SCOPE-ORACLE-hardened) on **1inch AggregationRouterV6**, Ethereum mainnet only.
> Treated as a fresh onboarding (no prior 1inch artifacts existed).
>
> STEP 0 verdict (verify-first): the existing `amm` ActionBody **already models a pool-less
> aggregator swap** — `AmmVenue::AggregatorRoute { chain, router, route_hash }` +
> `AggregatorKind::OneInchV6` + `SwapParams` + a fully-tested `lower_amm_venue` arm. So **NO
> Tier-3** is needed for the flagship swap. The single new primitive is a general-purpose
> `keccak256(bytes)→bytes32` `$fn` (for `route_hash` over 1inch's opaque `data` arg; the
> existing `route_hash` `$fn` only packs an `address[11]` Curve route).

## Run Metadata

| field | value |
|---|---|
| protocol | 1inch |
| branch | feat/1inch-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-1inch |
| date | 2026-06-03 |
| main agent | Claude Opus 4.8 (1M context), this session |
| base commit | c9916daf (feat/registry-v2) |

## Scope Classification

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | **Ethereum mainnet (`1`) ONLY.** The v6 router shares the canonical `0x1111...2a65` address on Arbitrum/Base/BSC/Polygon/Optimism but those are a SEPARATE multichain framework (explicit defer). |
| completion target | `wallet-facing` — AggregationRouterV6 swap surface. Round 1: `swap(executor,SwapDescription,data)` (`0x07ed2379`). **Round 2 (P3 follow-up, 2026-06-03): + `clipperSwap` (`0xd2d374e5`) + `clipperSwapTo` (`0xc4d652af`)** — **3 mutating selectors**, all decoded → `Amm::Swap` on `AggregatorRoute(OneInchV6)`. **Round 3 (P1 follow-up, 2026-06-03): + 1inch LOP v4** — the EIP-712 `Order` maker-sign (typed-data -> `Amm::SignIntentOrder`) + `cancelOrder` (`0xb68fb020`) / `cancelOrders` (`0x89e7c650`) -> `Amm::CancelIntentOrder` on the new `OneInchLimitOrder` venue. See the **Follow-up Round** sections below. |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | **P2-MEASURED, to=router selector distribution (users call the router directly, so tx.to IS the entrypoint — standard tx.to measurement, no router/direct discrimination needed):** `swap` (0x07ed2379) = **~70.5% (Dune 30d, 82,215 / 116,556 success txs)** and **~61.6% (Etherscan 10k most-recent, 5,082 / 8,253)** of v6-router state-changing txs. Two windows bracket it: **swap is the dominant single action, ~62–70%.** **P6 re-measure (Dune query 7641966, 30d, 0.59 cr): with P1 (LOP cancel) + P3 (Clipper) now COVERED, the covered on-chain set (swap + cancelOrder + cancelOrders + clipper) = 103,832 / 116,800 = ~88.9% of v6-router success txs (swap 82,299 + cancelOrder 20,952 + cancelOrders 581 + clipper 0).** **Unique-EOA view (the honest C1 lens): swap = 26,410 distinct EOAs (the retail surface) vs cancelOrder = 878 EOAs (~24 tx/EOA = MM cancellation bots) — so tx-count overstates cancelOrder; by users swap dominates and the LOP-cancel coverage captures the small MM-maker set.** |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | **LOP limit-order (cancel/fill)** ≈ **19.0% (30d)** — cancelOrder alone 17.9% (heavy MM cancellation traffic); the maker surface (`Order` sign + cancelOrder 17.9% + cancelOrders) is **NOW COVERED** (Round 3 / P1: new `OneInchLimitOrder` venue + typed-data Order sign + on-chain cancel). Only the taker/resolver-side `fillOrder*` remains deferred (not the wallet maker; decodable later as SettleIntentOrder). **unoswap family** ≈ **4.1% (30d)** (token_out-from-packed-pool $fn gap). **permitAndCall** ≈ 0.2% (permit+self-call recursion). **clipperSwap/clipperSwapTo** ≈ **0%** (0 occurrences in the original 2.3d/30d windows — negligible) — **NOW COVERED** (Round 2 / P3 follow-up via the `address_from_uint256` $fn; the surface is closed even though usage is ~0%). EXCLUDE-by-category: epoch bookkeeping (increaseEpoch/advanceEpoch) ≈ 6.2% (maker order-series invalidation, not a swap value/permission decision). |
| direct factory-child calls | not applicable — 1inch is a singleton router, not a factory/pool-heavy protocol (no child address universe). |
| final claim label (MUST NOT over-claim the measured coverage-share above) | see **Final Completion Claim** — bounded to the measured `swap`-selector share; explicitly *not* "the full 1inch swap surface" (unoswap/clipper/LOP deferred). |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification above — **mainnet (1) ONLY**, wallet-facing single-selector (`swap`) subset, multichain explicitly deferred. SCOPE CONTRACT fixed before P1 (pre-authorized by the onboarding goal: COVER swap, DEFER unoswap/clipper/LOP). |
| Codex current-session research executed | done | this session (Claude Opus) direct 1st-party verify: Etherscan v2 `getsourcecode` on `0x111111125421ca6dc452d289314280a0f8842a65` → ContractName **AggregationRouterV6** (verified source `AggregationRouterV6.mainnet.sol`); `getabi` → 50 functions (33 external-mutating); `cast sig` computed every selector (swap=0x07ed2379). v5/v4 ContractNames verified too (AggregationRouterV5 0x1111...0582, AggregationRouterV4 0x1111...097d). |
| Claude Code or sub-agent research executed | done | dispatched an independent general-purpose discovery sub-agent (id af7ad710) to enumerate 1inch mainnet user-facing contracts from 1st-party sources (portal.1inch.dev, github.com/1inch, Etherscan ContractName). Returned a full report + VERDICT. |
| Claude/sub-agent exact prompt or command recorded | done | sub-agent prompt embedded in this session's Agent call: "enumerate 1inch Ethereum-mainnet user-facing contracts … 1st-party only … is AggregationRouterV6 the complete user-facing swap surface? standalone LOP v4? Fusion settlement user-facing? older routers? spot-price aggregator?" Read-only, candidate-only. |
| Codex-only candidates listed | done | my direct Etherscan pass surfaced the v6 router full ABI (swap/unoswap*/clipperSwap*/fillOrder/cancelOrder/admin/callbacks) + v5/v4 router addresses. |
| Claude/sub-agent-only candidates listed | done | sub-agent additionally surfaced: legacy **standalone** LimitOrderProtocol V2 `0x119c71d3bbac22029622cbaec24854d3d32d2828` + V3-era `0x3ef51736315f52d568d6d2cf289419b9cfffe782`; Fusion Settlement `0xa88800cd…` (non-user-facing); SpotPriceAggregator `0x07d91f5f…` (non-user-facing); and the key fact that LOP v4 + Fusion share the v6 router's EIP-712 domain (name "1inch Aggregation Router", version "6", verifyingContract = v6 router). |
| dropped-unverified candidates listed with reason | done | Fusion 2.0 SettlementExtension / WhitelistRegistry — sub-agent could NOT 1st-party-pin a mainnet address (flagged UNVERIFIED); dropped (resolver-only, non-user-facing regardless). The "V3" version label on `0x3ef5…` is deploy-age inference (Etherscan labels it plainly "LimitOrderProtocol") — recorded as such, not over-asserted. |
| final contract inventory verified against first-party sources | done | AggregationRouterV6 `0x1111…2a65` (chain 1) Etherscan-verified ContractName + verified source. v5/v4/LOP-V2/LOP-V3 Etherscan ContractName verified (all `exclude`). `check:surface` I0 = `✓ 1inch: 5 deployed · 1 cover · 4 exclude`. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | **not applicable** — 1inch is a singleton router (1 cover contract), not a factory/pool/vault-heavy protocol. No `_address_universe.json`. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | **not applicable** (singleton router; no child universe). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | **not applicable** (no child universe). |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | **not applicable** (singleton router — one concrete `chain_to_addresses` entry, no factory children). |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | **not applicable** (no factory children). The router itself IS the entrypoint; `swap` covered. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | **not applicable** (no universe artifact for a singleton router). |
| token-surface inventory completed or explicitly scoped out | done | **scoped out** — 1inch is a pure aggregator/router; it mints NO LP/share/receipt/debt token. A swap's token_in/token_out are arbitrary external ERC-20s referenced by address from calldata (resolved by the `tokens:erc20` standard adapter / token_metadata capability), not protocol tokens. No `registryV2/tokens` entries added. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | `surface/1inch/_deployments.json` — 5 contracts: AggregationRouterV6 (cover) + V5/V4/LOP-V2/LOP-V3 (exclude, version expansion). |
| `npm run check:surface` output recorded | done | `PASS` (exit 0); `✓ AggregationRouterV6 [1]: 33 surface · 1 cover · 32 exclude · 1 on-chain manifests`; `✓ [I0] 1inch: 5 deployed · 1 cover · 4 exclude`. (First P0 run before the P1 manifest existed showed the expected `✗ I2 cover selector 0x07ed2379 (swap) has NO manifest`; resolved once the swap manifest landed.) |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | `swap` (0x07ed2379) → `AmmAction::Swap(SwapAction)` with `AmmVenue::AggregatorRoute{chain,router,route_hash}` (OneInchV6). Existing ActionBody — NO Tier-3 (STEP 0 verified `lower_amm_venue` aggregator_route arm + conformance test already shipped). |
| permission/fund-movement/red-flag selector review recorded | done | `swap` = fund-movement (sells token_in for token_out); policy-relevant fields token_in/token_out/amount/minReturn/recipient all decoded. The `executor` (whitelist-relevant per AggregatorMeta) is calldata arg 0 but the static `AggregatorRoute` venue has no executor slot (executor lives in the enrichment `route.aggregator`) — recorded as a known static limitation in round 1, **RESOLVED in the P2 follow-up** (an additive `executor: Option<Address>` slot was added to `AggregatorRoute` and the swap manifest now statically decodes it; see Follow-up Round 3). No permission-grant selector is COVER this round; `permitAndCall` (permit wrapper) is explicitly DEFER (not silently excluded) per the surface README "never-exclude permit primitive" rule. |
| manifest files added/changed listed | done | **1 manifest:** `registryV2/manifests/1inch/aggregation-router-v6/swap@1.0.0.json` (single_emit, AggregatorRoute venue, route_hash via `$fn keccak256` over `$args.data`, params from `desc[0/1/3/4/5]`). |
| enrichment/live_field decision recorded for every COVER action | done | swap params (token_in/out, amount_in, min_amount_out, recipient) are user-legible from calldata → no enrichment for the policy decode. `live_inputs` (route/expected_amount_out/price_impact_bp = `derived_from` skeleton `oneinch_v6_*`; gas_estimate = pyth oracle_feed) mirror Curve router-ng — deferred enrichment (the executed route lives in the opaque `data`, resolvable only via the 1inch API; dormant policy-RPC path does not fetch). |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | live_inputs are skeletons (decode does not fetch; policy-RPC dormant). No live handler required for the decode/verdict path. Enrichment calc_ids (`oneinch_v6_route`/`_expected_out`/`_price_impact_bp`) named for future API wiring (VenueApi `api.1inch.dev/swap/v6.0/1/swap`, parser `oneinch_v6_route` — already referenced in the shipped lowering test). |
| Tier3 not needed or full Tier3 downstream contract completed | done | **Tier3 NOT needed.** The amm Swap ActionBody + AggregatorRoute venue + OneInchV6 AggregatorKind + lowering arm were already shipped and conformance-tested. The only new primitive is a general `$fn keccak256` (not a Tier-3 ActionBody/domain/venue change). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | **not applicable (no Tier3).** New primitive = `crates/adapters/mappers/src/declarative/builtin_fn.rs` — added `keccak256` to WHITELIST + dispatch + `keccak256_hex()` impl + unit test `keccak256_hashes_bytes_deterministically_to_32_bytes` (10/10 builtin_fn tests pass). No Cedar/schema/effect/view/sync change (Swap already registered). |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `check:manifest` exit 0: `validate (all): 1483 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24, source-ref representative]` — the 1inch swap manifest fuzz-decodes into a valid SwapAction. build-index: **52975 callkeys / 83 typed-data / 815 manifests** (+1 swap callkey vs base; callkey `1__0x1111…2a65__0x07ed2379` present). |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `v3-harness fuzz --filter 1inch --iterations 5000 --seed 0x31696e6368` → `total=5000 pass=5000 soft=0 fail=0 panic=0`; domain histogram `amm 100%`. |
| iterations >= 5000 or justified lower bound | done | 5000 requested; 1inch routable surface = 1 callkey (swap) → 5000 swap decodes, 0 failures. |
| fixed edge-case matrix recorded | done | fuzz boundary values (0, U256::MAX) per callkey; corpus adds natural real edges: native-ETH sentinel (0xEeee…) as src and dst, very-small (amount=30689) and very-large (87e18) amounts, distinct dstReceiver. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | `swap` calldata is structurally FLAT — `(address, static 7-field tuple, bytes)` — so no nested-recurse/array_emit/opcode-stream/deadline/path edges apply. Value edge = native-sentinel swap with msg.value>0 (NATIN corpus entry). Permission edge = N/A on swap (no grant); permitAndCall (the permit wrapper) is DEFER not silently excluded. Deferred-selector edge = unoswap route-miss (corpus `expect:error`). |
| representative pass/error corpus entries committed or justified | done | `data/golden/v3-decode/1inch/corpus.json` — **11 entries**: 8 ERC20→ERC20 swaps (USDC/USDT/WBTC/PAXG/XAUt/…) + 2 native-sentinel swaps (ETH→token, token→ETH) all `pass` with field pins; 1 `unoswap` entry `expect:error` (deferred selector must route-miss, no mis-decode). |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 (`ETHERSCAN_API_KEY` in `crates/integration-tests/.env`), chain 1 — `getsourcecode`/`getabi`/`txlist` all work. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | `account&action=txlist&address=0x111111125421ca6dc452d289314280a0f8842a65&offset=10000&sort=desc` (adapter-blind by P0 cover address = the router). |
| external tx pull target address count is nonzero and recorded | done | **1 target** (AggregationRouterV6 — the singleton cover contract). Nonzero. |
| Etherscan `api_calls_used` recorded | done | ~5 calls: getsourcecode ×3 (v6/v5/v4) + getabi-via-getsourcecode (v6) + txlist ×1 (10k). |
| Etherscan `raw_txs_seen` recorded | done | 10,000 rows (blocks 25215042→25231416, ~2.3 days); 8,253 success txs with to=router (1,747 isError=1 dropped). |
| Etherscan `unique_selectors_seen` recorded | done | 16 distinct selectors in the 2.3d sample (20 in the Dune 30d window): swap, cancelOrder, increaseEpoch, unoswap/unoswap2/3, ethUnoswap/To*, cancelOrders, fillOrder/Args, fillContractOrder/Args, permitAndCall, advanceEpoch, unoswapTo/2. |
| Etherscan real tx coverage per COVER selector recorded | done | COVER selector `swap` (0x07ed2379) = **5,082 real txs** observed (61.6% of 8,253 success txs to router). |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | the router IS the wallet-facing target (users call it directly). **1 target**, floor 10k pulled, 8,253 success / 5,082 swap matched. target file = `surface/1inch/_deployments.json` (AggregationRouterV6 cover). |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | non-swap = actionable-but-DEFERRED: cancelOrder 2,093 + cancelOrders 50 + fillOrder/fillContractOrder ~43 (LOP, ~26.5% 2.3d), unoswap family 420 (~5.1%), permitAndCall 16. non-actionable EXCLUDE: increaseEpoch 529 + advanceEpoch 20 (maker epoch bookkeeping ~6.7%). No mis-routed unknown. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | **not applicable** — 1inch is a singleton router, no factory/pool/vault child universe. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | none — txlist is keyed on the router address; every selector seen is in the v6 ABI and triaged in coverage.json. No unknown-address-with-1inch-selector gap. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | **not applicable this round** — the only EIP-712 surface is the LOP v4 / Fusion `Order` (domain name "1inch Aggregation Router" v6, verifyingContract = the v6 router; confirmed by discovery sub-agent), which is **DEFERRED** (coverage.json `signed_structs.Order` = exclude/DEFER). No in-scope typed-data primaryType this round. |
| Dune MCP/API availability checked | done | Dune MCP available (`getUsage`, `createAndExecuteQuery`, free engine). |
| Dune usage baseline recorded | done | baseline **413.812 / 2500** credits (billing 2026-05-05 → 2026-06-05, plan community_fluid_engine_v2). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | query **7639563** (30d selector distribution; partition WHERE `block_time >= now() - interval '30' day`, success, length(data)≥4, free engine), 21 rows. (query 7639555 first attempt FAILED on `cardinality(varbinary)` → fixed to `length()`.) |
| Dune `executionCostCredits` / usage delta recorded | done | 0.52 credits (7639563); failed 7639555 not charged. |
| Dune rows returned / selected tx hashes recorded | done | 30d query: 20 selector rows, 116,556 total txs. Corpus tx hashes (11) selected from the Etherscan txlist pull (real swap/native/unoswap hashes embedded in corpus.json). |
| representative real-tx corpus/golden entries committed or justified | done | `data/golden/v3-decode/1inch/corpus.json` — 11 entries (10 pass swap incl. 2 native-sentinel + 1 unoswap error). |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter 1inch --require-expect-body` → **11/11 matched, 10/10 pass entries pinned** (token_in=desc[0], token_out=desc[1], amount_in=desc[4], min_amount_out=desc[5], recipient=desc[3], venue.name=aggregator_route, venue.router=$to all verified on real mainnet calldata); unoswap `expect:error got:error` (deferral proof). |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | **MEASURED at tx.to (users call the router directly).** Covered `swap` (0x07ed2379) = **70.5% (Dune 30d, 82,215/116,556)** / **61.6% (Etherscan 2.3d, 5,082/8,253)** of v6-router success txs. DEFER usage-shares (30d): LOP cancel/fill **19.0%** (#1 follow-up), unoswap family **4.1%**, permitAndCall 0.2%, clipper **0%**. EXCLUDE: epoch bookkeeping 6.2%. Completion label bounded to ~62–70% swap-selector share (NOT "the full 1inch swap surface"). |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | **NO decode gap on the covered surface** — corpus 10/10 pinned + fuzz 5000/5000 + 0 structural errors. All buckets are DEFERS, data-gated with measured usage-share: (1) LOP cancel/fill ~19.0% (#1); (2) unoswap family ~4.1% ($fn/token_out-from-pool); (3) permitAndCall ~0.2% (recursion); (4) clipper ~0%. EXCLUDE: epoch bookkeeping ~6.2%, callbacks, admin. No `unknown_protocol_address` (txlist address-keyed to the router; every selector triaged). |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | No decode fixes were needed this onboarding (clean first-pass decode). The one new primitive (`keccak256` $fn) is tied to the COVER selector `swap` (0x07ed2379) `route_hash` requirement, validated by corpus 0x56ab0a… (+9 more) and builtin_fn unit test. |
| manifest/decoder/Tier3/harness change list recorded | done | **manifest:** `manifests/1inch/aggregation-router-v6/swap@1.0.0.json` (new). **decoder:** `builtin_fn.rs` — new `keccak256` $fn (no Tier3/ActionBody/cedarschema change; AggregatorRoute venue pre-existing). **harness:** `data/golden/v3-decode/1inch/corpus.json` (new, 11 entries). **surface:** `surface/1inch/{_deployments,aggregation-router-v6.abi,aggregation-router-v6.coverage}.json`. |
| P2 rerun after fixes recorded | done | no fixes → no rerun needed. Re-validation after fmt: `corpus --filter 1inch --require-expect-body` 11/11 matched / 10/10 pinned; fuzz 5000/5000; `cargo test --workspace --exclude policy-engine-integration-tests` (see P4). |
| corpus `expect` flips or exclusions justified | done | no flips. Every `expect:pass` correct on first decode (pins matched first run); the 1 `expect:error` (unoswap) is a deliberate deferral proof (route miss), not a flip. |
| remaining gaps have explicit defer/blocker disposition | done | all DEFERs above carry a 1st-party usage-share (data-gated). LOP/unoswap/clipper/permitAndCall = explicit DEFER in coverage.json with reasons. Multichain = separate framework. No silent gaps. |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | `done — 52975 callkey(s) + 83 typed-data entr(ies) across 815 manifest(s)` (+1 swap callkey `1__0x1111…2a65__0x07ed2379` vs base 52974/814). (One ENOTEMPTY on `index/by-callkey` from iCloud-synced stray files → `rm -rf index && npm run build` clean; index is gitignored/generated.) |
| registryV2 build-index vitest output recorded | done | toolchain provisioned this session: `cd browser-extension && node .yarn/releases/yarn-4.14.1.cjs install` (904 pkgs) then `node .yarn/releases/yarn-4.14.1.cjs vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts` → **Test Files 1 passed (1), Tests 12 passed (12)** (4.9s). Closed in the P6 follow-up (Round 6). |
| `npm run check:manifest` output recorded | done | exit 0: `validate (all): 1483 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24, source-ref representative]`. |
| `npm run check:surface` output recorded | done | exit 0; `✓ AggregationRouterV6 [1]: 33 surface · 1 cover · 32 exclude · 1 on-chain manifests`; `✓ [I0] 1inch: 5 deployed · 1 cover · 4 exclude`. Only pre-existing unrelated WARNs (morpho I0', aave/compound ungated). |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | **not applicable** — 1inch is a singleton router (no `_address_universe.json`; not pool/factory/vault-heavy). |
| v3-harness coverage/fuzz/corpus outputs recorded | done | fuzz `--filter 1inch --iters 5000 --seed 0x31696e6368` → 5000/5000 pass (amm 100%); corpus `--filter 1inch` → 11/11 matched. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter 1inch --require-expect-body` → **11/11 matched, 10/10 pass entries pinned**; unoswap `expect:error got:error`. |
| `cargo test --workspace` output recorded | done | `cargo test --workspace --exclude policy-engine-integration-tests` → **exit 0, 0 failed** (all core crates: policy-action/transition/engine/sync/state/mappers/… + doc-tests). integration-tests `v3_decode_harness` full golden = covered for the changed surface by `corpus --filter 1inch --require-expect-body` (11/11) + the **full cross-protocol `v3-harness corpus` replay (no filter) → 345/345 matched** (all protocols incl. the new 11 1inch entries decode correctly) — proves the keccak256 $fn (additive WHITELIST arm) regressed no existing decoder; full golden test-suite bounded per FW-1 (`--test-threads`). |
| wasm build output recorded if runtime/wasm/schema changed | done | `./scripts/wasm-build.sh` → release compile 1m48s (mappers w/ new keccak256 $fn + policy-engine-wasm), wasm-bindgen + wasm-opt `✨ Done`, artifact copied to `browser-extension/backend/wasm/` + `public/wasm/`. Optimized `policy_engine_wasm_bg.wasm` = 10,721,691 bytes (~10.7 MiB). Required because the keccak256 $fn (mappers) runs in the WASM declarative-decode path. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | `cargo fmt -p mappers -- --check` clean (after auto-fmt of the new `keccak256_hex` — only builtin_fn.rs touched). `cargo clippy -p mappers --all-targets -- -D warnings` = clean (exit 0, no warnings). |
| exact staged files and commit hash recorded | done | Onboarding landed across 4 explicit-stage commits on `feat/1inch-onboarding`: **d109f545** P0 (surface/1inch/{_deployments,abi,coverage}.json + evidence), **65800825** P1 (builtin_fn.rs keccak256 $fn + manifests/1inch/aggregation-router-v6/swap@1.0.0.json), **89aa8d5f** P2 (data/golden/v3-decode/1inch/corpus.json + evidence), and the **P3/P4 commit carrying this evidence** (staged: builtin_fn.rs fmt-wrap + this evidence.md; HEAD of git log). Generated `index/`, `pkg/`, `browser-extension/**/wasm/` gitignored. `.cargo/config.toml` (target-dir redirect) git-ignored, never staged. |
| remaining WARNs/deferred selectors/actions listed with reason | done | Deferred (data-gated): LOP cancel/fill ~19% (#1 follow-up — needs 1inch-LOP IntentVenue + AddressLib unmask), unoswap family ~4.1% ($fn/token_out-from-pool gap), permitAndCall ~0.2% (recursion), clipper ~0%. EXCLUDE: epoch bookkeeping ~6.2%, callbacks, admin. Separate framework: multichain (Arbitrum/Base/BSC/Polygon/Optimism), legacy v5/v4 routers + standalone LOP V2/V3. WARNs: pre-existing registry-wide (morpho I0', aave/compound ungated) — not 1inch. |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | see Final Completion Claim — bounded to the measured ~62–70% `swap`-selector coverage-share; explicitly NOT "the full 1inch swap surface". |
| no base/worktree merge performed unless user explicitly requested it | done | no merge/push; all work on `feat/1inch-onboarding` (worktree scopeball-1inch). Shared base `feat/registry-v2` untouched. |

## Blockers

| blocker | source | next action |
|---|---|---|
| route / price-impact / expected-out enrichment is dormant (Track 2, platform-level — NOT an onboarding decode blocker) | the v6 swap `live_inputs` (route / expected_amount_out / price_impact_bp) are `derived_from` skeletons that the policy-RPC / Sync orchestrator does not yet fetch (`oneinch_v6_*` calc_ids named, not wired) | out of Track-1 scope — every Track-1 decode is complete and statically self-contained (the swap envelope token_in/out/amount/minReturn/recipient/executor is all from calldata). Wiring the 1inch API (route quality, live executor route metadata) is a separate platform round. The two round-1 onboarding blockers are both RESOLVED: `executor`-slot in P2 (Round 3), build-index vitest toolchain in P6 (Round 6). |

## Final Completion Claim

**Onboarding status: COMPLETE (mainnet-only — `swap` + Clipper + 1inch LOP v4 maker surface [Order sign + cancel] subset), bounded by measured coverage.**

> **wallet-facing, Ethereum mainnet (`1`) ONLY: 1inch AggregationRouterV6 (`0x111111125421ca6dc452d289314280a0f8842a65`) `swap(executor,SwapDescription,data)` (`0x07ed2379`) → `Amm::Swap` on `AggregatorRoute(OneInchV6)`.** No Tier-3 (the AggregatorRoute venue + OneInchV6 kind + lowering were pre-shipped); two additive decode primitives — `keccak256` `$fn` (route_hash) + `address_from_uint256` `$fn` (AddressLib unmask of Clipper's packed `srcToken`). **Plus Clipper:** `clipperSwap` (`0xd2d374e5`) / `clipperSwapTo` (`0xc4d652af`) on the same AggregatorRoute venue (Round 2 / P3 follow-up, 2026-06-03).
>
> **Measured coverage (tx.to — users call the router directly):** `swap` = **~70.5% (Dune 30d, 82,215/116,556)** / **~61.6% (Etherscan 2.3d, 5,082/8,253)** of successful v6-router state-changing txs — the **dominant single action**. Validated on 10 real mainnet swaps (8 ERC20→ERC20 + 2 native-ETH-sentinel), all field-pinned. This is **NOT "the full 1inch swap surface"** — see deferrals. **P6 update (Round 6): with the P1 LOP-cancel + P3 Clipper coverage, the covered on-chain set is now ~88.9% of v6-router txs (swap + cancelOrder + cancelOrders); by unique-EOA, swap = 26,410 EOAs (retail) vs cancelOrder 878 EOAs (MM cancel bots).**
>
> **Deferred (data-gated, 1st-party usage-share measured):** 1inch **Limit Order Protocol v4** fill/cancel + the off-chain EIP-712 `Order` signature ≈ **19.0% (30d)** — the maker surface (`Order` sign + cancelOrder/cancelOrders, ~18%) is **now COVERED** in Round 3 / P1 (new `OneInchLimitOrder` `IntentVenue` variant + typed-data Order sign + on-chain cancel; `maker_traits_expiry`/`coalesce_address` $fns); only the taker/resolver-side `fillOrder*` stays deferred (not the wallet maker; decodable later as SettleIntentOrder); **unoswap family** ≈ **4.1%** (token_out-from-packed-pool `$fn` gap, enrichment not wired); **permitAndCall** ≈ 0.2% (permit + self-call recursion). (**clipperSwap/To** ≈ 0% — **no longer deferred; COVERED in Round 2 / P3 follow-up**.) **EXCLUDE:** epoch bookkeeping ≈ 6.2% (maker order-series invalidation), callbacks, admin. **Separate framework/rounds:** multichain v6 (Arbitrum/Base/BSC/Polygon/Optimism, shared `0x1111…2a65`); legacy AggregationRouterV5/V4 + standalone LOP V2/V3.
>
> Deferred selectors with no manifest correctly **route-miss → warn-closed** (safe default; proven by the unoswap corpus `expect:error` entry).

## Follow-up Round — Clipper coverage + `address_from_uint256` $fn (2026-06-03, P3)

> Second round on the same branch (`feat/1inch-onboarding`). Closes the **Clipper** surface
> — the only remaining *calldata-decodable* mutating selectors — and lands the general
> AddressLib-unmask `$fn` that the LOP follow-up (P1) will reuse for packed `Order` fields.
> Coverage-share is essentially unchanged (~62–70%) because Clipper is ~0% of v6-router
> traffic, but the non-LOP / non-unoswap mutating surface is now fully closed and the
> reusable `address_from_uint256` `$fn` is landed + tested on real mainnet calldata.

| item | status | artifact / summary |
|---|---|---|
| new `$fn` `address_from_uint256(uint256)->address` | done | `crates/adapters/mappers/src/declarative/builtin_fn.rs` — WHITELIST + dispatch + impl (low-160-bit unmask via `U256::to_be_bytes::<32>()[12..]`) + `json_u256` helper + unit test `address_from_uint256_unmasks_low_160_bits` (mappers builtin_fn **12/12** pass). **Rust-only gate** (build-index `validateEmitShape` checks only `emit.strategy`, NOT `$fn` names) — the stale doc comment that claimed it mirrors the WHITELIST was corrected in this commit. Reused by P1 (LOP `Order` maker/makerAsset/takerAsset are AddressLib-packed uint256). |
| Clipper manifests (exclude→cover flip) | done | `manifests/1inch/aggregation-router-v6/clipper-swap@1.0.0.json` (`0xd2d374e5`) + `clipper-swap-to@1.0.0.json` (`0xc4d652af`) — single_emit `AggregatorRoute` swap; `token_in = address_from_uint256($args.srcToken)`, `token_out = $args.dstToken`, `recipient = $tx.from` (clipperSwap → msg.sender) / `$args.recipient` (clipperSwapTo), `route_hash = keccak256($args.clipperExchange)`. `surface/1inch/aggregation-router-v6.coverage.json` flipped both selectors exclude(DEFER)→cover → now `33 surface · 3 cover · 30 exclude · 3 on-chain manifests`. |
| real-tx corpus (Clipper) | done | 2 real mainnet entries appended to `data/golden/v3-decode/1inch/corpus.json` (now **13**): clipperSwap DAI→USDC `0x62f77df8…6dd154`, clipperSwapTo WETH→DAI `0xf8a1aa23…bd5abd670`. Real txs found via **Dune** query `7641475` (v6-router selector scan, 2024-01-01→now, small engine, **13.6 credits**); calldata fetched via **Etherscan** `eth_getTransactionByHash`. Both `pass`, fully field-pinned — proving `address_from_uint256` unmasks the packed `srcToken` uint256 to the real ERC-20 (DAI / WETH). |
| gates (Round 2) | done | `corpus --filter 1inch --require-expect-body` → **13/13 matched, 12/12 pinned**; full cross-protocol `corpus` (no filter) → **347/347 matched** (was 345; +2 Clipper; the new $fn regressed no existing decoder); `check:surface` → **PASS** (`1inch: 3 cover · 3 manifests`); `check:manifest` → **1485 single_emit OK, 0 structural errors**; `cargo test --workspace --exclude policy-engine-integration-tests` → **0 failed**; `cargo fmt -p mappers --check` + `cargo clippy -p mappers --all-targets -- -D warnings` clean; `./scripts/wasm-build.sh` OK (the new $fn compiles to wasm32). |
| still deferred (unchanged this round) | done | **LOP** cancel/fill + EIP-712 `Order` ≈ **19%** (P1 — the #1 follow-up; reuses `address_from_uint256`); **unoswap** family ≈ 4.1% (token_out-from-packed-pool $fn gap, enrichment not wired); **permitAndCall** ≈ 0.2% (permit + self-call recursion). **Separate framework:** multichain v6; legacy v5/v4 + standalone LOP V2/V3. |
| executor static slot (P2) | done | still a documented follow-up (see Blockers) — `AmmVenue::AggregatorRoute` carries no `executor` slot; the calldata `executor` (swap arg0) is statically decodable and P2 will add an additive `executor: Option<Address>` venue field. Not addressed in this Clipper round (Clipper has no executor arg). |

## Follow-up Round 2 — 1inch Limit Order Protocol v4 (P1, 2026-06-03)

> Third round. Covers the **maker-facing LOP v4 surface** — the single largest deferred 1inch
> surface (~18-19% of v6-router txs): the off-chain EIP-712 `Order` signature + on-chain
> `cancelOrder`/`cancelOrders`. Adds a precise `IntentVenue::OneInchLimitOrder` variant (NOT the
> Fusion venue) + two reusable `$fn`s. Taker-side `fillOrder*` stays excluded (resolver, not the
> wallet maker). Design ExitPlanMode-approved (Option B: new variant over reusing OneInchFusion).

| item | status | artifact / summary |
|---|---|---|
| new `IntentVenue::OneInchLimitOrder { chain, verifying_contract }` | done | `crates/policy-server/asset-model/action/src/amm/intent.rs` + compile-enforced fan-out: `name()` arm (`one_inch_limit_order`), `lower_intent_venue` (emits `{name, chain, verifyingContract}`), `project_venue` (spender = verifying_contract = v6 router — better than Fusion's `Address::ZERO`), sync `intent_chain_id`, `view.rs` all-5-variants assert, Cedar `IntentVenue` record `verifyingContract?` field. New **venue**, NOT a new action -> the Cedar 3-site action registration is untouched. Conformance: `sign_intent_venue_one_inch_limit_order_conforms` (lowering -> Cedar schema validate). The tsify `IntentVenue` TS binding auto-regenerates via wasm-build (gitignored, not staged). |
| two new `$fn`s | done | `crates/adapters/mappers/src/declarative/builtin_fn.rs`: `maker_traits_expiry(uint256)->uint` (MakerTraits bits 80..120 = uint40 absolute unix-seconds expiry; on-chain `0`=never-expires is remapped to the max-uint40 sentinel so a never-expiring order reads as a far-future `valid_until`, not epoch-0) + `coalesce_address(addr,fallback)->address` (LOP `Order.receiver==0` means the maker is the recipient). + unit tests (`mappers` builtin_fn **14/14**). 1st-party: github.com/1inch/limit-order-protocol v4 `MakerTraitsLib.sol` / `OrderLib.sol`. |
| 3 manifests | done | `manifests/1inch/limit-order-protocol/`: **`order-sign@1.0.0.json`** (typed-data; domain '1inch Aggregation Router' v6, verifying_contract = the v6 router, primaryType `Order`; the EIP-712 `Order` declares maker/asset fields as `address` so the message resolves them directly — `address_from_uint256` is NOT needed on the sign path; `recipient = coalesce_address($args.order.receiver, $args.order.maker)`, `valid_until = maker_traits_expiry($args.order.makerTraits)`), **`cancel-order@1.0.0.json`** (single_emit, `0xb68fb020`, `order_hash = $args.orderHash`), **`cancel-orders@1.0.0.json`** (array_emit over `orderHashes` bytes32[], bare `$inputs` scalar element -> multicall of CancelIntentOrder, `0x89e7c650`). Mirrors `uniswapx/v3-dutch-order/sign` + `pendle/limit-router/sign-limit-order`. |
| surface (cover flips) | done | `coverage.json`: `signed_structs.Order` + `cancelOrder` (`0xb68fb020`) + `cancelOrders` (`0x89e7c650`) flipped exclude->cover; the `fillOrder`/`fillOrderArgs`/`fillContractOrder`/`fillContractOrderArgs` reasons corrected to taker/resolver-side (maker surface now covered; fill decodable later as SettleIntentOrder). AggregationRouterV6 now `33 surface · 5 cover · 28 exclude · 5 on-chain manifests · 1 signed-struct`. |
| real-tx corpus (3 entries) | done | `data/golden/v3-decode/1inch/corpus.json` (now 19 entries): **cancelOrder** `0x0d225caa` (order_hash pinned), **cancelOrders** `0x1ebed9bf` (array_emit -> multicall; BOTH child order_hashes pinned at `body.actions[N].order_hash`; expect_domain=multicall), **Order sign** (typed-data; real `Order` extracted from fillOrder `0xb4d057de`: sell=WETH, buy=0x68749665, **receiver=0 -> recipient=maker via coalesce_address**, **valid_until=1780453531 via maker_traits_expiry** — all pinned). Real txs via Dune query `7641643` (4.2 credits); calldata via Etherscan `eth_getTransactionByHash`. |
| gates (Round 3) | done | `corpus --filter 1inch --require-expect-body` **16/16 matched, 15/15 pinned**; full cross-protocol `corpus` **350/350** (was 347; +3 LOP; no regression); `cargo test --workspace --exclude policy-engine-integration-tests` **0 failed** (new variant compiles + conformance passes); `check:surface` PASS; `check:manifest` **1486 single_emit OK, 0 structural errors**; `cargo fmt` clean + `cargo clippy --workspace --all-targets -- -D warnings` clean; `./scripts/wasm-build.sh` OK (new variant + $fns compile to wasm32; tsify binding regenerated). |
| still deferred (after P1) | done | taker/resolver-side `fillOrder*` (small share; decodable later as SettleIntentOrder); **unoswap** family ~4.1% (token_out-from-packed-pool $fn gap); **permitAndCall** ~0.2%. **executor static slot** (P2 — see Blockers). Multichain = separate framework. |

## Follow-up Round 3 — executor static slot (P2, 2026-06-03)

> Closes the round-1 Blocker: the 1inch v6 `executor` (swap calldata arg 0, the contract the
> router delegates the swap to) is now **statically decoded into the venue**, so a policy can
> whitelist known-safe executors pre-sign — instead of relying on the dormant route-enrichment
> `AggregatorMeta.executor`.

| item | status | artifact / summary |
|---|---|---|
| `AmmVenue::AggregatorRoute.executor: Option<Address>` (additive) | done | `crates/policy-server/asset-model/action/src/amm/mod.rs` — new optional field (`serde(default, skip_serializing_if)` + `tsify(optional)`, mirroring `AggregatorMeta.executor`). `lower_amm_venue` (`lowering_v2/amm/mod.rs`) emits `executor` when `Some`; the Cedar `AmmVenue` record gains `executor?: String`. 7 construction sites updated (`executor: None`, or `Some` in the conformance test). SHARED venue (Uniswap/Curve/Aero) → additive only; every non-1inch AggregatorRoute manifest stays `None` (no regression). |
| swap manifest + corpus | done | `swap@1.0.0.json` venue gains `"executor": "$args.executor"` (calldata arg 0). The first swap corpus entry pins `venue.executor` (`0x4c3ccc98…a6e3`, real). Clipper manifests carry no executor (Clipper has no such arg). |
| gates (Round 4) | done | 1inch corpus **16/16** (executor pinned); full corpus **350/350** (no regression); `cargo test --workspace --exclude policy-engine-integration-tests` **0 failed** (7 construction sites compile + `swap_venue_aggregator_route_conforms` with `executor: Some` validates against the Cedar `executor?` field); `check:manifest` **1486 OK**; `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` clean; `./scripts/wasm-build.sh` OK. |
| note | done | Defense-in-depth: the swap's fund-safety is already bounded by `minReturnAmount`/`dstReceiver`; the executor slot adds an explicit, statically-decodable handle for executor-allowlist policies. The dormant enrichment `AggregatorMeta.executor` (live route metadata) is a separate, platform-dependent concern (Track 2). |

## Follow-up Round 4 — native-sentinel + dstReceiver edges (P4, 2026-06-03)

> Two swap-decode edge refinements: (1) the native-asset sentinel (`0xEeee…EEeE`) now decodes
> to `TokenKey::Native` instead of an erc20-with-sentinel key, so a "limit native ETH spend"
> policy keys on Native; (2) `dstReceiver == 0` (the router sends output to msg.sender) now
> resolves the recipient to the sender instead of emitting `0x0`.

| item | status | artifact / summary |
|---|---|---|
| new `$fn` `token_key_or_native(address, chain)` | done | `crates/adapters/mappers/src/declarative/builtin_fn.rs` — maps the 1inch native sentinel (`0xeeee…eeee`) to `TokenKey::Native { chain }`, any other address to `TokenKey::Erc20 { chain, address }`. The only `$fn` that returns a JSON OBJECT (the token `key`), not a scalar. + unit test (`mappers` builtin_fn **15/15**). |
| swap manifest | done | `swap@1.0.0.json`: `token_in`/`token_out` keys are now `{"$fn":"token_key_or_native","$args":["$args.desc[0\|1]","$chain"]}` (native-aware); `recipient` is now `{"$fn":"coalesce_address","$args":["$args.desc[3]","$tx.from"]}` (reuses the P1 `coalesce_address` $fn — `dstReceiver==0` resolves to msg.sender). |
| corpus | done | the 2 native swap entries now pin `token_in\|out.key.standard == "native"` (was the sentinel address); the 8 ERC-20 entries (non-sentinel) still pin `key.address` (erc20). recipient pins unchanged (real dstReceiver non-zero → coalesce returns it). |
| gates (Round 5) | done | 1inch corpus **16/16** (15/15 pinned, incl. both native entries → Native); full corpus **350/350** (no regression — non-1inch swap manifests unaffected); `cargo test --workspace --exclude policy-engine-integration-tests` **0 failed**; `check:manifest` **1486 OK**; `cargo fmt` + `cargo clippy -p mappers --all-targets -- -D warnings` clean; `./scripts/wasm-build.sh` OK. |
| scope note | done | Applied to the dominant `swap` path (where native ETH is common). Clipper keeps the erc20 key (native-via-Clipper ≈ 0% of an already ~0% surface; would need a nested `token_key_or_native(address_from_uint256(...))` — a documented minor edge, not wired this round). |

## Follow-up Round 5 — SCOPE ORACLE re-measure + build-index vitest (P6, 2026-06-03)

> Process-honesty pass. (C1) Re-measures the covered coverage-share now that P1 (LOP cancel) +
> P3 (Clipper) are covered, and adds the unique-EOA lens that de-weights MM cancellation bots.
> (C3) Closes the long-standing build-index vitest blocker by provisioning the browser-extension
> toolchain in this session.

| item | status | artifact / summary |
|---|---|---|
| C1 — SCOPE ORACLE re-measure (tx-count + unique-EOA) | done | Dune query **7641966** (per-selector `count(*)` + `count(distinct "from")`, v6 router, 30d, 0.59 cr). **Covered on-chain set = swap + cancelOrder + cancelOrders + clipper = 103,832 / 116,800 = ~88.9%** of success txs (up from ~70.5% swap-only at onboarding). **Unique-EOA: swap = 26,410 distinct EOAs (retail surface); cancelOrder = 878 EOAs (~24 tx/EOA = MM cancellation bots)** — confirming tx-count overstates cancelOrder; by users, swap dominates and LOP-cancel coverage captures the small MM-maker set. Deferred (tx-count): unoswap family 4.06% + fillOrder-taker 0.64% + permitAndCall 0.20%. EXCLUDE: epoch bookkeeping 6.21%. |
| C3 — build-index vitest | done | `cd browser-extension && node .yarn/releases/yarn-4.14.1.cjs install` (904 pkgs, network OK this session) then `node .yarn/releases/yarn-4.14.1.cjs vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts` → **Test Files 1 passed (1), Tests 12 passed (12)** (4.9s). The round-1 "blocked — toolchain not provisioned" disposition is now CLOSED. |
| honest framing | done | The completion label is bounded to the measured covered set; the unique-EOA lens is reported alongside tx-count so MM-bot inflation of cancelOrder is explicit, not hidden. The only remaining limitation is the Track-2 enrichment (route / price-impact / expected-out, platform-level) — out of Track-1 onboarding scope (see Blockers). |

Verify:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- 1inch --phase all
```

## Follow-up Round 6 — FULL-COVERAGE (NativeOrderFactory + unoswap family + universe re-audit + PowerPod) — 2026-06-03

> Triggered by a dogfood miss: a real `NativeOrderFactory.create` tx route-missed (`bundle_not_installed` → warn-closed).
> Isolated worktree `scopeball-1inch-full` / branch `feat/1inch-full-coverage` (off `5d6710f7`); commit `454bcf79` (NOT merged/pushed).

| track | status | summary |
|---|---|---|
| **A NativeOrderFactory** | done | `0xe12e0f11…ff01` `create((uint256x8)=IOrderMixin.Order)` (`0x8c72b608`) → `Amm::SignIntentOrder` (OneInchLimitOrder venue, order_kind=limit) — on-chain twin of order-sign, `address_from_uint256` on AddressLib-packed fields, NO engine change. surface(abi+coverage) + create@1.0.0 + real-tx golden `0x418c7157`. NativeOrderImpl CREATE2 clone (cancel/withdraw) = child universe → defer. Gotcha: single-tuple param flattens to top-level args; SignIntentOrder/Swap require emit-level `live_inputs`. |
| **B unoswap family (12)** | done | token_out is the pool's other token (needs token0()/token1() read) → NOT static. Model change `SwapParams.token_out: TokenRef → Option<TokenRef>` (lowering omits when None; effect skips output credit; Cedar `tokenOut?`; ~25 construction sites Some-wrapped + None-lowering test). New `unoswap_route_hash` $fn. 12 single_emit `Amm::Swap` AggregatorRoute(OneInchV6) manifests (all args uint256; token_out omitted). coverage flip exclude→cover. real-tx goldens unoswap `0x0aa9564d` + ethUnoswap `0xda46df75`. Rejected Track-B build-time pool-baking (doesn't fit router+calldata-pool shape) and TokenKey::Unknown (80-file blast). |
| **C universe re-audit** | done | `_deployments.json` 5→13 rows. Prior version-axis enumeration missed separate product lines. Triaged: NativeOrderFactory (cover), NativeOrderImpl (defer), st1INCH (defer), PowerPod (cover), + explicit exclude rows (1INCH token / Fusion Settlement / EscrowFactory / SpotPriceAggregator). Diagnosed 3 enumeration blind spots. |
| **PowerPod** | done | `0xaccfac23…` `delegate(delegatee)` (`0x5c19a95c`) → `Permission::ProtocolAuthorization{permission=delegate}` (reuse, no model change). surface + manifest + real-tx golden `0x55f5e34e`. |
| **st1INCH staking** | defer | deposit/withdraw/earlyWithdraw need a new non-Curve `StakingVenue` = focused round; addPod/removePod delegation deferred with it. ABI snapshotted. |

**Gates (all green):** `check:surface` PASS (AggregationRouterV6 **17 cover** [+12 unoswap] · NativeOrderFactory 1 · PowerPod 1 · no regression); `v3-harness corpus --filter 1inch --require-expect-body` **19/19 matched, 19/19 pinned**; build-index **904 manifests** (12 unoswap callkeys, `unoswap_route_hash` passed the $fn gate); `cargo test` (policy-action/transition/engine/mappers) 0 failed; fmt+clippy clean. Reverted 5 base-fmt-dirty churn files (`cargo fmt --all` incidental; metamorpho_underlying = parallel-session file).

**Deferred (post-merge):** GCS republish + extension rebuild — feature branch; publishing unmerged adapters to the live bucket would be premature.
