# Protocol Onboarding Evidence — lido

> Completion gate, not a note. Status column must be exactly `done` or `blocked`
> (parser-strict). Run `cargo run -p policy-engine-integration-tests --bin
> check-onboarding-evidence -- lido --phase <p0|p1|p2|p3|p4|all>` per phase.
>
> **Run mode:** ♻️ re-verification. Lido was partially pre-onboarded (12 manifests
> + surface/lido + liquid_staking domain committed at the base commit). Per the
> kickoff directive this run re-executes the framework P0→P4 from scratch, treating
> every pre-existing artifact as a **candidate verified against first-party
> sources**, not trusted. New + re-verification converge on the same gates.

## Run Metadata

| field | value |
|---|---|
| protocol | lido |
| branch | feat/lido-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-lido |
| date | 2026-06-03 |
| main agent | Claude Opus 4.8 (1M) |
| base commit | c9916daf |

## Scope Classification

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | Ethereum mainnet (`eip155:1`). L2 wstETH bridge deployments = multichain → deferred (separate framework). |
| completion target | `wallet-facing` |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | pending (P2 SCOPE ORACLE) |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | None on the representative chain. Only DEFER = L2/multichain wstETH (categorical multichain defer, exempt from within-chain usage-share). |
| direct factory-child calls | not applicable — Lido is a fixed-contract protocol (3 singleton/proxy contracts), not factory/pool-heavy. |
| final claim label (MUST NOT over-claim the measured coverage-share above) | pending (set at P4 against P2 coverage-share) |

**COVER (3 wallet-facing contracts, mainnet):**
- stETH (Lido, Aragon proxy) `0xae7ab96520de3a18e5e111b5eaab095312d7fe84` — submit / transferShares / transferSharesFrom
- wstETH (immutable) `0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0` — wrap / unwrap
- WithdrawalQueueERC721 (unstETH, OssifiableProxy) `0x889edc2edab5f40e902b864ad4d7ade8e412f9b1` — requestWithdrawals(+WstETH/+WithPermit/+WstETHWithPermit) / claimWithdrawal(s)(To)

**EXCLUDE (infra/governance — by definition not user pre-sign, usage-share-exempt):** Lido impl, Staking Router, Lido Locator, Accounting, Withdrawal Vault, Accounting Oracle, Validators Exit Bus Oracle, Lazy Oracle, DAO Kernel, LDO token (standard ERC-20), Dual Governance, Emergency Protected Timelock. Standard ERC-20/721 `approve`/`transfer`/`transferFrom`/`permit`/`setApprovalForAll` on the 3 cover tokens → excluded to the erc20/erc721 standard adapter.

**Known decode edges (carry to P3):** (1) bare-ETH stake — sending ETH with empty calldata to stETH (`Lido` fallback, `NON_EMPTY_DATA`-guarded) or to wstETH (`receive()`, stakes+wraps) has **no 4-byte selector** → not routable by the `(chain,to,selector)` decoder → fail-closed `warn` (conservative). (2) lido.fi stake-widget `to` is **UNVERIFIED** (P0 researcher caveat) — if the official widget routes stake through a referral wrapper instead of `stETH.submit`, that wrapper is an uncovered wallet-facing `to`; P2 real-tx must resolve.

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification above: rep chain `eip155:1`, target `wallet-facing`, multichain (L2 wstETH) deferred. |
| Codex current-session research executed | done | Main session (Claude Opus 4.8) re-verified the pre-existing surface/manifests against first-party sources: reconciled the 3-contract inventory vs `surface/lido/_deployments.json`, cross-checked every coverage triage decision vs the verified inventory, and confirmed addresses vs docs.lido.fi. |
| Claude Code or sub-agent research executed | done | `protocol-researcher` sub-agent (agentId `aa8b151858ce62cad`): full first-party inventory of stETH / wstETH / WithdrawalQueueERC721 external functions + EIP-712 typed data, with live on-chain confirmation (eip712Domain / implementation slots) and Foundry `cast` selector/typehash computation. |
| Claude/sub-agent exact prompt or command recorded | done | Prompt dispatched to `protocol-researcher`: "RESEARCH-ONLY … enumerate every external/public state-changing function + EIP-712 typed data for stETH `0xae7ab9…fE84`, wstETH `0x7f39…2Ca0`, WithdrawalQueueERC721 `0x889e…F9B1` on Ethereum mainnet; first-party sources only (docs.lido.fi / Etherscan verified / lidofinance GitHub); per-function signature+selector+intent table + typed-data domains + completeness check." (full prompt in session transcript) |
| Codex-only candidates listed | done | Main-session-only findings: the bare-ETH-stake edge (empty-calldata, no selector — invisible to the selector-keyed surface gate); stale "New … (Tier 3)" wording in coverage reasons (the liquid_staking domain now exists — cosmetic, left as-is to avoid churn). |
| Claude/sub-agent-only candidates listed | done | Researcher-only: `increaseAllowance`/`decreaseAllowance` selectors; stETH EIP-712 domain `version="2"` (non-default) gotcha; exact `PermitInput` tuple order `(value,deadline,v,r,s)`; wstETH `receive()` stake-and-wrap shortcut; lido.fi referral-wrapper caveat (UNVERIFIED). |
| dropped-unverified candidates listed with reason | done | (1) lido.fi stake-widget `to` — UNVERIFIED, deferred to P2 real-tx (resolve whether dominant stake `to` is `stETH.submit` or a wrapper). (2) Implementation addresses behind the proxies (`0x6ca8…`, `0xe42c…`) — point-in-time, not keyed on; the decoder keys on `(chain, proxy-address, selector)`. |
| final contract inventory verified against first-party sources | done | 3 cover addresses match docs.lido.fi/deployed-contracts + on-chain proxy/impl reads (researcher). `surface/lido/_deployments.json` = 15 contracts (3 cover / 12 exclude) vs docs.lido.fi. check:surface I0 enforced. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | not applicable — Lido is a fixed-contract protocol (3 singleton/proxy contracts); no factory/pool/registry child universe. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | not applicable (no factory/pool universe). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | not applicable (no child universe). |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | not applicable — all 3 contracts are concrete fixed addresses with concrete per-`(chain,address,selector)` manifests. |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | not applicable (no factory children). |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable — Lido is not pool/factory/vault-heavy; no `_address_universe.json`/`_pool_universe.json`. |
| token-surface inventory completed or explicitly scoped out | done | `registryV2/tokens/1/`: stETH (`0xae7ab9…`, erc20, kind=wrapped/rebasing, underlying native), wstETH (`0x7f39…`, erc20, kind=wrapped, underlying stETH), unstETH NFT (`0x889e…`, erc721, kind=stake_receipt, underlying native — **added this run**), ETH native sentinel present. `npm run check:tokens -- --chain 1` → PASS (0 errors; unstETH clean). |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | `surface/lido/_deployments.json` — 15 contracts, 3 cover / 12 exclude, source `docs.lido.fi/deployed-contracts` (verified current). |
| `npm run check:surface` output recorded | done | `npm run check:surface` → PASS. stETH 32 surface / 3 cover / 29 exclude / 3 manifests; wstETH 8 / 2 cover / 6 exclude / 2 manifests; WithdrawalQueueERC721 23 / 7 cover / 16 exclude / 7 manifests; I0 lido 15 deployed / 3 cover / 12 exclude (contract-inventory enforced vs docs.lido.fi). |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | 12 cover selectors → `LiquidStakingAction` variants: submit→`stake`; wrap→`wrap`; unwrap→`unwrap`; requestWithdrawals/+WstETH/+WithPermit/+WstETHWithPermit→`request_withdrawal`; claimWithdrawal/s/To→`claim_withdrawal`; transferShares/From→`transfer_shares`. Domain + all 6 sub-actions already exist (`crates/policy-server/asset-model/action/src/liquid_staking/{stake,wrap,unwrap,request_withdrawal,claim_withdrawal,transfer_shares}.rs`) — **no new Tier3 needed**. |
| permission/fund-movement/red-flag selector review recorded | done | submit = fund-IN (user ETH `msg.value` → stETH; `referral` informational). transferShares/From = share-denominated fund-MOVE (From spends allowance). request* = fund-MOVE (burns stETH/wstETH, mints NFT to `_owner` — **`owner` is a redirect red-flag field, captured in body**). claim*To = fund-OUT ETH (**`recipient` redirect red-flag, captured**). WithPermit variants embed an EIP-2612 allowance grant whose spender = the queue itself (bounded/self-contained); the permit `value`/`deadline` are decoded in ABI but not surfaced in body (modeling note — amounts ARE surfaced). Standard ERC20/721 approve/permit/transfer/setApprovalForAll excluded → analyzed by erc20/erc721 standard adapters (the permission primitives those adapters exist to flag). |
| manifest files added/changed listed | done | ♻️ verification — 0 manifests changed. 12 pre-existing manifests verified: `manifests/lido/steth/{submit,transferShares,transferSharesFrom}@1.0.0.json`, `manifests/lido/wsteth/{wrap,unwrap}@1.0.0.json`, `manifests/lido/withdrawal-queue/{requestWithdrawals,requestWithdrawalsWstETH,requestWithdrawalsWithPermit,requestWithdrawalsWstETHWithPermit,claimWithdrawal,claimWithdrawals,claimWithdrawalsTo}@1.0.0.json`. (New file this run = unstETH token only, recorded under P0 token-surface.) |
| enrichment/live_field decision recorded for every COVER action | done | Enriched (onchain_view live_input → host shows concrete value behind the abstract unit): wrap→`getWstETHByStETH` (decoder_id `lido_wsteth_by_steth`); unwrap→`getStETHByWstETH` (`lido_steth_by_wsteth`); transferShares/From→`getPooledEthByShares` (`lido_pooled_eth_by_shares`). Faithful static decode, enrichment deferred (live={}): stake, request_withdrawal, claim_withdrawal. Decision matches `liquid_staking/mod.rs` design comment. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | 3 onchain_view enrichment methods are plumbed end-to-end in the ActionBody + `lowering_v2/liquid_staking/{wrap,unwrap,transfer_shares}.rs` LiveField (a `v3_decode_harness` test asserts the `getWstETHByStETH` live_field survives action_builder→reducer-struct→lowering). Runtime materialization depends on the policy-RPC host, which is **dormant framework-wide** (no configured endpoint in default config; verdicts are 100% local WASM Cedar). Disposition: declared + plumbed; runtime fetch deferred (framework-level dormant RPC, not a Lido gap); static decode is faithful without it. |
| Tier3 not needed or full Tier3 downstream contract completed | done | Tier3 NOT needed — liquid_staking domain + 6 sub-actions + lowering_v2 + cedarschema + schema registration + conformance all pre-exist and pass `cargo test --workspace` (verified at base). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | not applicable (no new Tier3 this run). Pre-existing downstream: action `liquid_staking/`, effect/view/sync, `lowering_v2/liquid_staking/`, `schema/policy-schema/actions/liquid_staking/**`, conformance — all present at base. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `cargo run -q --bin v3-harness -- validate --filter lido` → **12 single_emit manifest(s) OK, 0 structural errors** (iters/manifest=24). Every lido manifest's emit.body matches the typed ActionBody struct via the production decoder. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `cargo run --bin v3-harness -- fuzz --iterations 5000 --seed 42 --filter lido` → total=60000 pass=60000 soft=0 fail=0 panicked=0 skipped=0; domain histogram 100% liquid_staking. |
| iterations >= 5000 or justified lower bound | done | 5000 iters/callkey × 12 lido callkeys = 60000 routed inputs. |
| fixed edge-case matrix recorded | done | Per-callkey the fuzzer runs `EDGE_ITERS` boundary inputs (zero / max-uint / boundary) before random; plus the 9-entry real-tx corpus pins per-intent edges: single (`claimWithdrawal`) vs batch (`claimWithdrawals`) request_ids[], single-element amounts[], stETH-vs-wstETH token discriminator (request vs requestWstETH), and embedded-permit variants (WithPermit / WstETHWithPermit). |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | value = `submit` amount=$tx.value (0 and max swept); array = `amounts[]` (request*) and `request_ids[]` (claim*); token-discriminator = stETH vs wstETH (request variants). N/A for lido (flat single_emit, no Tier-3 sub-structure): nested/opcode/deadline/path (no multicall, opcode-stream, deadline, or swap-path); permission (no lido-specific permission primitive — NFT approve/setApprovalForAll + ERC-2612 permit → standard adapters). |
| representative pass/error corpus entries committed or justified | done | 9 real-mainnet pass entries (`data/golden/v3-decode/lido/corpus.json`), one per cover intent (submit/wrap/unwrap/requestWithdrawals/+WstETH/+WithPermit/+WstETHWithPermit/claimWithdrawal/claimWithdrawals), each now field-level `expect_body`-pinned. No error entries: all 12 cover selectors decode `pass`; the only non-pass real cases are (a) spam/misdirected calldata (documented in the Etherscan sweep, not a decoder error) and (b) the selectorless bare-ETH stake edge (an orchestrator warn-closed case, nothing for the WASM decoder to decode). |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | `ETHERSCAN_API_KEY` (len 34) in `crates/integration-tests/.env` (local-only); v2 API sanity call to stETH txlist → `status=1 message=OK`. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | `node crates/integration-tests/scripts/etherscan-bulk-sweep.mjs --protocol lido` (target-source=deployments → the 3 cover addresses); output `onboarding/lido/etherscan-bulk-summary.json`. |
| external tx pull target address count is nonzero and recorded | done | coverAddressesQueried = 3 (stETH, wstETH, WithdrawalQueueERC721). |
| Etherscan `api_calls_used` recorded | done | apiCallsUsed = 3. |
| Etherscan `raw_txs_seen` recorded | done | rawTxsSeen = 30000 (10k/target × 3; floorMet=true vs 20k floor); inputTxsSeen = 29983 (17 empty-calldata bare-ETH sends excluded — the selectorless stake edge). |
| Etherscan `unique_selectors_seen` recorded | done | uniqueSelectorsSeen = 26 (matched 13 / excluded 10 / unmatched 4). |
| Etherscan real tx coverage per COVER selector recorded | done | All 12 lido cover selectors observed direct: submit 3517, claimWithdrawals 4490, requestWithdrawalsWithPermit 3612, unwrap 1465, requestWithdrawals 1263, wrap 1224, requestWithdrawalsWstETHWithPermit 369, claimWithdrawal 221, requestWithdrawalsWstETH 10, claimWithdrawalsTo 3; transferShares/transferSharesFrom 0 in this 30k sample (low-volume, covered). |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | The 3 cover contracts ARE the wallet-facing targets (no separate router). target count 3, per-target floor 10k, rawTxs 30000, matchedInputTxs 29875, target file `registryV2/surface/lido/_deployments.json`. |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | unmatchedInputTxs = 7 → actionable 0 / non-actionable 7: `non_abi_or_text_calldata` 5 (selectors 0x4555d5c9, 0x7d031b65 — absent from target ABIs, spam/probe) + `selector_known_elsewhere_wrong_target` 2 (0x3ccfd60b = Curve `withdraw()` misdirected to the unstETH queue). **0 actionable = no missing lido decoder.** |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | not applicable — Lido is not factory/pool-heavy; the 3 cover contracts ARE the universe (swept in full). |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | None. Sweep is keyed by tx.to ∈ {3 cover addrs}; the one cross-protocol selector seen (0x3ccfd60b, Curve withdraw) was misdirected TO the unstETH queue → bucketed non-actionable wrong-target, not a lido gap. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | not applicable — Lido exposes no protocol-specific EIP-712 type. The only off-chain sigs on lido tokens are standard ERC-2612 `permit` (stETH `Liquid staked Ether 2.0` v"2" / wstETH `Wrapped liquid staked Ether 2.0` v"1") — `signed_structs`=0 in coverage, decoded by the generic EIP-2612 path, out of Lido's gate scope (same treatment as on-chain `approve` → erc20 standard adapter). On-chain `*WithPermit` variants embed the permit in calldata (not typed-data) and ARE covered (manifests + corpus). |
| Dune MCP/API availability checked | done | `mcp__dune getUsage` OK; plan `community_fluid_engine_v2`. |
| Dune usage baseline recorded | done | creditsUsed 414.707 / quota 2500 (billing period 2026-05-05 → 2026-06-05). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | Dune query 7639592 (free engine), `ethereum.traces` WHERE `block_date >= CURRENT_DATE - INTERVAL '7' DAY` (partition pruning) AND `"to"`=stETH AND `bytearray_substring(input,1,4)`=0xa1903eab — submit direct-vs-internal split. |
| Dune `executionCostCredits` / usage delta recorded | done | executionCostCredits = 0.664 (free engine). |
| Dune rows returned / selected tx hashes recorded | done | 2 rows (7d): direct_top_level submit = 1629 txs; internal_via_router_or_wrap = 365 txs. → 81.6% of stETH `submit` calls are direct top-level. |
| representative real-tx corpus/golden entries committed or justified | done | 9-entry real-mainnet corpus (`data/golden/v3-decode/lido/corpus.json`, one per cover intent) + 4 hand goldens in `v3_decode_harness.rs` (submit amount/referral, requestWithdrawals owner/token, wrap amount, wrap live-input wiring). Corpus upgraded to field-level `expect_body` this run. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `cargo run --bin v3-harness -- corpus --filter lido --require-expect-body` → corpus 9/9 matched; semantic expect_body 9/9 pass entries pinned; exit 0. |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | **Coverage-share = 29875/29983 = 99.64%** of recent direct input txs to the 3 cover contracts decode (lido manifest or erc20/721 standard adapter); **0 actionable uncovered**. Lido-specific surface = 16,394 txs, all decoded by lido manifests. Measurement unit = top-level tx (txlist tx.to ∈ cover set; inherently direct — no internal-call over-count). DEFER usage-share: only DEFER = multichain L2 (categorical, exempt). Internal-routing context (Dune 7d): 81.6% of stETH `submit` is direct (ScopeBall-decoded as Lido); 18.4% internal (wstETH `receive()` selectorless edge ~17/30k direct + external routers = their own surface). **Completion label bounded to "wallet-facing direct surface, 99.6%" — NOT "all Lido staking".** |

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
| (none blocking P0/P1) | — | P2-tracked open items: lido.fi stake-widget `to` (UNVERIFIED) + bare-ETH-stake selectorless edge — both resolved/dispositioned in P2/P3. |

## Final Completion Claim

Not complete. P0 + P1 are `done` and gate-checked; P2/P3/P4 pending. Final label set at P4, bounded by the P2 SCOPE ORACLE coverage-share.

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- lido --phase all
```
