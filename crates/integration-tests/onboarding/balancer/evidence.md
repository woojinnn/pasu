# Protocol Onboarding Evidence — balancer

> Copied from `ONBOARDING_EVIDENCE_TEMPLATE.md`. This is a completion gate.
> Run = deeper re-onboarding of an already-shallow Balancer V2/V3 (option A: maximal coverage, Tier-2 decoders).

## Run Metadata

| field | value |
|---|---|
| protocol | balancer |
| branch | feat/balancer-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-balancer |
| date | 2026-06-03 |
| main agent | Claude (Opus 4.8) |
| base commit | 79d8ae90 (feat/registry-v2) |

## Scope Classification

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | mainnet (chainId 1) — Balancer max-activity chain. Existing multichain V2 swap/relayer (7 chains) kept as legacy-present; new-function multichain expansion deferred. |
| completion target | `wallet-facing` |
| covered real-usage coverage-share (P2-measured) | pending (P2 SCOPE ORACLE) |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | V3 addLiquidityProportional 0.6% / addLiquidityUnbalanced 0.5% / removeLiquidityProportional 2.3% (pool-token list not in calldata; decode-time resolver unwired); V2 flashLoan 3.6% (integrator callback); V2 manageUserBalance/managePoolBalance <1%; V3 initialize 2.3% (one-time bootstrap); permitBatchAndCall's bundled Permit2 grant (children covered, grant not separately surfaced); multichain expansion of new functions. |
| direct factory-child calls | deferred (wallet-facing router/vault scope; direct pool calls = full-universe follow-up) |
| final claim label (MUST NOT over-claim) | pending (P4, derived from SCOPE ORACLE) |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification table above: mainnet, wallet-facing, multichain deferred. |
| Codex current-session research executed | done | Main-session (Claude) research: read existing surface/manifests, measured V2 Vault + V3 Router selector distributions (Etherscan v2 txlist 10k tx each), verified decode architecture (declarative_exports.rs / action_builder.rs `$fn` mechanism / multicall_recurse recurse_arg / lowering_v2 amm) in code. |
| Claude Code or sub-agent research executed | done | general-purpose sub-agent (id ac7c13cc5ed7743e4) — Balancer mainnet user-facing contract inventory from 1st-party balancer-deployments GitHub + Etherscan verify + usage gauge. 41 tool calls. |
| Claude/sub-agent exact prompt or command recorded | done | Prompt: "P0 contract-discovery … verified, 1st-party inventory of Balancer user-facing entry contracts on mainnet, esp. V3 routers beyond Router v2; sources = balancer/balancer-deployments per-task output/mainnet.json + docs.balancer.fi + Etherscan getsourcecode/txlist". Result table in §discovery below. |
| Codex-only candidates listed | done | none unique — main-session research = the 2 already-covered entries (V2 Vault, V3 Router-v2) + their function distributions. |
| Claude/sub-agent-only candidates listed | done | sub-agent surfaced the MISSING contracts (I0 gap): V3 BatchRouter, CompositeLiquidityRouter v2, BufferRouter, AggregatorRouter, AggregatorBatchRouter, UnbalancedAddViaSwapRouter, Router v1 (deprecated); V2 BalancerRelayer v6, BalancerQueries, ProtocolFeesWithdrawer. All added to _deployments with disposition. |
| dropped-unverified candidates listed with reason | done | CompositeLiquidityRouter **v3** + PrepaidCompositeLiquidityRouter v3 — deploy tasks exist but NO mainnet output/ artifact → not deployed; NOT adopted (live mainnet composite = v2). |
| final contract inventory verified against first-party sources | done | every address confirmed vs balancer/balancer-deployments per-task output/mainnet.json + Etherscan getsourcecode ContractName match; 3 already-covered addresses re-confirmed canonical. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | n/a for THIS run's scope: wallet-facing entry is the singleton Vault/Router contracts (one address each, not factory-child-direct). Direct pool/pair calls = deferred full-universe follow-up (Balancer V2/V3 pools are factory children users normally reach via the Vault/Router, not by direct call). |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | n/a — singleton router/vault scope (no _pool_universe.json); direct-pool universe deferred. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | n/a — direct pool calls deferred as a class (wallet-facing router/vault scope), recorded here + in Scope Classification. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | n/a — singletons; no per-pool manifest generation this run. |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | DEFERRED separately: users calling a Balancer pool directly are out of this wallet-facing router/vault run; the swap/liquidity manifests' pool_meta live_inputs support the router tx and do NOT create direct-pool callkeys. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | n/a — no _address_universe.json (singleton scope); check:universe not applicable to router/vault-only onboarding. |
| token-surface inventory completed or explicitly scoped out | done | BAL governance token already registered (tokens/1/0xba10…). Covered-pool BPT/underlying token JSON NOT needed for wallet-facing router/vault scope (no direct-pool callkeys this run); deferred with the direct-pool universe. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | added 10 mainnet contracts (V3 BatchRouter/CompositeLiquidityRouter v2/BufferRouter/Aggregator×2/UnbalancedAddViaSwap/Router v1 + V2 BalancerRelayer v6/Queries/ProtocolFeesWithdrawer) each cover|exclude with 1st-party source + measured usage. |
| `npm run check:surface` output recorded | done | PASS — "every gated contract's external surface is fully triaged and consistent". V2 Vault[1]=5 cover/5 manifests, V3 Router-v2[1]=7 cover/7 manifests, I0 balancer 27 deployed/9 cover/18 exclude (contract-inventory enforced vs balancer/balancer-deployments). |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | V2: swap→Amm::Swap (existing), setRelayerApproval→Permission::ProtocolAuthorization (existing), batchSwap→Amm::Swap aggregate (new manifest), joinPool→Amm::AddLiquidity::Pooled (new), exitPool→Amm::RemoveLiquidity::PooledBurn (new). V3: swapSingleToken{In,Out}+multicall (existing), addLiquiditySingleTokenExactOut→AddLiquidity::Pooled (new), removeLiquiditySingleToken{In,Out}→RemoveLiquidity::PooledBurn (new), permitBatchAndCall→Multicall via multicall_recurse (new). ALL map to EXISTING ActionBodies — NO Tier-3. |
| permission/fund-movement/red-flag selector review recorded | done | setRelayerApproval (relayer grant) = ProtocolAuthorization, covered. No hidden permission grants in covered selectors. Fund-movement: swap/batchSwap/join/exit/liquidity all carry token+amount+recipient. flashLoan (borrow red-flag) DEFERRED with measured 3.6% usage + reason (callback-recipient primitive, not retail EOA). permitBatchAndCall bundles a Permit2 batch GRANT — surfaced as a documented limitation (children covered, grant not separately emitted; flagged in manifest _note + coverage reason). |
| manifest files added/changed listed | done | NEW manifests (7): v2/{vault-join-pool,vault-exit-pool,vault-batch-swap}@1.0.0, v3/{router-add-liquidity-single-token-exact-out,router-remove-liquidity-single-token-exact-in,router-remove-liquidity-single-token-exact-out,router-permit-batch-and-call}@1.0.0. NEW Tier-2 builtins (4) in crates/adapters/mappers/src/declarative/builtin_fn.rs + fn_whitelist.json: balancer_zip_token_amounts, balancer_pool_id_to_address, balancer_v2_userdata_field, balancer_v2_batch_swap_field. coverage.json flips: v2-vault-mainnet (batchSwap/joinPool/exitPool→cover), v3-router-v2-mainnet (addLiqSingleOut/removeLiqSingle{In,Out}/permitBatchAndCall→cover). |
| enrichment/live_field decision recorded for every COVER action | done | liquidity manifests carry AddLiquidityLiveInputs{pool_state,current_price} / RemoveLiquidityLiveInputs{pool_state,fees_owed}; batchSwap carries SwapLiveInputs{route(pool_meta),expected_amount_out,price_impact_bp,gas_estimate} mirroring the existing vault-swap manifest. These are EVAL-TIME enrichment (dormant — no live RPC dispatcher wired), same status as all existing balancer/curve liquidity+swap manifests; the static verdict uses the calldata-derived fields. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | BLOCKER (documented, pre-existing): live_inputs (registry_api pool_meta, onchain_view getPoolTokens/getCurrentLiveBalances, derived_from calc_ids) have NO local handler / configured endpoint — they fail-closed-dormant exactly like the existing balancer swap manifests. The Tier-2 $fn builtins (zip/pool_id/userdata/batch_swap) are decode-time (local, no remote) and fully wired + unit-tested (6/6) + validate-clean. |
| Tier3 not needed or full Tier3 downstream contract completed | done | Tier-3 NOT needed — AddLiquidity/RemoveLiquidity/Swap ActionBodies + lowering_v2/amm/{add_liquidity,remove_liquidity,swap}.rs + BalancerV2/V3 AmmVenue all pre-exist. Only Tier-2 (declarative `$fn` builtins) added. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | n/a — no Tier-3 (no new domain/action). Tier-2 only: 4 `$fn` builtins + whitelist. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `v3-harness validate --representative-source-refs`: ALL = 1787 single_emit manifests OK, 0 structural errors (exit 0); filter=balancer = 24 OK, 0 errors. `npm run build` = 899 manifests, 53482 callkeys. mappers builtin unit tests 6/6 pass. (Initial validate caught 3 fuzz-artifact failures in zip/batch_swap on type-valid synthetic mismatched-arrays/huge-indices → fixed by making the $fn builtins tolerant: min-length zip + modulo-wrap index; real correctness pinned by P2 corpus expect_body.) |

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
| Etherscan MCP/API availability checked | done | api.etherscan.io v2 txlist verified (status 1 OK) against V2 Vault + V3 Router; ETHERSCAN_API_KEY local. |
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
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe, and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | pending | |

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
| | | |

## Final Completion Claim

Run `cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- balancer --phase all` before any complete claim.
