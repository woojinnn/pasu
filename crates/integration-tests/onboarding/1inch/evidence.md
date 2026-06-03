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
| completion target | `wallet-facing` — AggregationRouterV6 swap surface. Round 1: `swap(executor,SwapDescription,data)` (`0x07ed2379`). **Round 2 (P3 follow-up, 2026-06-03): + `clipperSwap` (`0xd2d374e5`) + `clipperSwapTo` (`0xc4d652af`)** — **3 mutating selectors**, all decoded → `Amm::Swap` on `AggregatorRoute(OneInchV6)`. See the **Follow-up Round** section below. |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | **P2-MEASURED, to=router selector distribution (users call the router directly, so tx.to IS the entrypoint — standard tx.to measurement, no router/direct discrimination needed):** `swap` (0x07ed2379) = **~70.5% (Dune 30d, 82,215 / 116,556 success txs)** and **~61.6% (Etherscan 10k most-recent, 5,082 / 8,253)** of v6-router state-changing txs. Two windows bracket it: **swap is the dominant single action, ~62–70%.** |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | **LOP limit-order (cancel/fill)** ≈ **19.0% (30d)** — cancelOrder alone 17.9% (heavy MM cancellation traffic); the largest deferred surface, **#1 follow-up** (needs a 1inch-LOP IntentVenue + AddressLib unmask of packed Order fields). **unoswap family** ≈ **4.1% (30d)** (token_out-from-packed-pool $fn gap). **permitAndCall** ≈ 0.2% (permit+self-call recursion). **clipperSwap/clipperSwapTo** ≈ **0%** (0 occurrences in the original 2.3d/30d windows — negligible) — **NOW COVERED** (Round 2 / P3 follow-up via the `address_from_uint256` $fn; the surface is closed even though usage is ~0%). EXCLUDE-by-category: epoch bookkeeping (increaseEpoch/advanceEpoch) ≈ 6.2% (maker order-series invalidation, not a swap value/permission decision). |
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
| permission/fund-movement/red-flag selector review recorded | done | `swap` = fund-movement (sells token_in for token_out); policy-relevant fields token_in/token_out/amount/minReturn/recipient all decoded. The `executor` (whitelist-relevant per AggregatorMeta) is calldata arg 0 but the static `AggregatorRoute` venue has no executor slot (executor lives in the enrichment `route.aggregator`) — recorded as a known static limitation (see Blockers / Final Claim). No permission-grant selector is COVER this round; `permitAndCall` (permit wrapper) is explicitly DEFER (not silently excluded) per the surface README "never-exclude permit primitive" rule. |
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
| registryV2 build-index vitest output recorded | blocked | browser-extension Yarn 4 / WASM toolchain not provisioned in this onboarding worktree (same as other onboarding worktrees). build correctness covered by `npm run build` (validates every manifest+token) + `check:manifest` (1483 OK) + `check:surface`. Rerun: `cd browser-extension && yarn && yarn vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts`. |
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
| `executor` (calldata arg 0, executor-whitelist-relevant) has no slot in the static `AggregatorRoute` venue | the venue carries `{chain, router, route_hash}`; executor lives in the enrichment `live_inputs.route.aggregator.executor` (a LiveField, dormant) | non-fatal — the policy-relevant swap envelope (token_in/out/amount/minReturn/recipient/router) decodes statically; executor-whitelist policies need the route enrichment wired (1inch API), a documented follow-up. Not a decode error. |

## Final Completion Claim

**Onboarding status: COMPLETE (mainnet-only — `swap` + Clipper `clipperSwap`/`clipperSwapTo` subset), bounded by measured coverage.**

> **wallet-facing, Ethereum mainnet (`1`) ONLY: 1inch AggregationRouterV6 (`0x111111125421ca6dc452d289314280a0f8842a65`) `swap(executor,SwapDescription,data)` (`0x07ed2379`) → `Amm::Swap` on `AggregatorRoute(OneInchV6)`.** No Tier-3 (the AggregatorRoute venue + OneInchV6 kind + lowering were pre-shipped); two additive decode primitives — `keccak256` `$fn` (route_hash) + `address_from_uint256` `$fn` (AddressLib unmask of Clipper's packed `srcToken`). **Plus Clipper:** `clipperSwap` (`0xd2d374e5`) / `clipperSwapTo` (`0xc4d652af`) on the same AggregatorRoute venue (Round 2 / P3 follow-up, 2026-06-03).
>
> **Measured coverage (tx.to — users call the router directly):** `swap` = **~70.5% (Dune 30d, 82,215/116,556)** / **~61.6% (Etherscan 2.3d, 5,082/8,253)** of successful v6-router state-changing txs — the **dominant single action**. Validated on 10 real mainnet swaps (8 ERC20→ERC20 + 2 native-ETH-sentinel), all field-pinned. This is **NOT "the full 1inch swap surface"** — see deferrals.
>
> **Deferred (data-gated, 1st-party usage-share measured):** 1inch **Limit Order Protocol v4** fill/cancel + the off-chain EIP-712 `Order` signature ≈ **19.0% (30d)** — the single highest-value follow-up (needs a 1inch-LOP `IntentVenue` variant + AddressLib unmask of packed `Order` uint256 fields + typed-data routing); **unoswap family** ≈ **4.1%** (token_out-from-packed-pool `$fn` gap, enrichment not wired); **permitAndCall** ≈ 0.2% (permit + self-call recursion). (**clipperSwap/To** ≈ 0% — **no longer deferred; COVERED in Round 2 / P3 follow-up**.) **EXCLUDE:** epoch bookkeeping ≈ 6.2% (maker order-series invalidation), callbacks, admin. **Separate framework/rounds:** multichain v6 (Arbitrum/Base/BSC/Polygon/Optimism, shared `0x1111…2a65`); legacy AggregationRouterV5/V4 + standalone LOP V2/V3.
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

Verify:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- 1inch --phase all
```
