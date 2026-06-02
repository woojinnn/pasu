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
| completion target | `wallet-facing`, **single-selector subset** — AggregationRouterV6 `swap(executor,SwapDescription,data)` (`0x07ed2379`) only, decoded → `Amm::Swap` on `AggregatorRoute(OneInchV6)`. |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | **P2-MEASURED (Dune, mainnet, to=router selector distribution):** see P2 SCOPE ORACLE row. `swap` (0x07ed2379) share of v6-router state-changing txs = _<filled in P2>_. |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | **unoswap family** (12 selectors; token_out-from-packed-pool $fn gap), **clipperSwap/clipperSwapTo** (packed srcToken uint256), **LOP v4 fillOrder/cancelOrder + EIP-712 Order sign** (packed addresses + makerTraits + new IntentVenue), **permitAndCall** (permit+self-call recursion). Each share P2-measured (SCOPE ORACLE row). |
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
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | pending | |

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

| blocker | source | next action |
|---|---|---|
| `executor` (calldata arg 0, executor-whitelist-relevant) has no slot in the static `AggregatorRoute` venue | the venue carries `{chain, router, route_hash}`; executor lives in the enrichment `live_inputs.route.aggregator.executor` (a LiveField, dormant) | non-fatal — the policy-relevant swap envelope (token_in/out/amount/minReturn/recipient/router) decodes statically; executor-whitelist policies need the route enrichment wired (1inch API), a documented follow-up. Not a decode error. |

## Final Completion Claim

_(filled at P4 — bounded to the P2-measured `swap`-selector coverage-share; must not over-claim.)_

Verify:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- 1inch --phase all
```
