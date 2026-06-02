# Protocol Onboarding Evidence Template

> Copy this file to `crates/integration-tests/onboarding/<protocol>/evidence.md` for each protocol onboarding run.
> This is a completion gate, not a nice-to-have note. If any mandatory row is missing, the phase is incomplete.
>
> **SSOT:** this template is the single source of truth for *per-phase evidence requirements* — `check-onboarding-evidence` parses it and cross-checks every mandatory row. The spine's §2.1b table, §3.1 P0 step, and §8.6 self-check summarize it; on conflict, this file wins. (The definition of "onboarded" itself lives in `PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md` §6.)

## Run Metadata

| field | value |
|---|---|
| protocol | cowswap |
| branch | feat/cowswap-onboarding |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-cowswap |
| date | 2026-06-03 |
| main agent | Claude Opus 4.8 (1M context) |
| base commit | c9916daf |

## Scope Classification

Use this section to make the final claim precise. This table is narrative
evidence; the phase tables below are the mandatory gate.

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | Ethereum mainnet (chainId 1) ONLY. Gnosis/Arbitrum/Base/Sepolia variants deferred. |
| completion target | `wallet-facing` — the off-chain EIP-712 `Order` signature surface (CowSwap's dominant gasless-intent UX) + on-chain order cancellation. |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | P2-measured (see P2 SCOPE ORACLE row) |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | `setPreSignature`, `CoWSwapEthFlow.createOrder`, `ComposableCoW.create/setRoot` — each P2 usage-share (see P2 SCOPE ORACLE) |
| direct factory-child calls | not applicable (CowSwap is a settlement singleton, not factory/pool/vault-heavy) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | (set in P4) |

### SCOPE CONTRACT (declared before P1 — pre-authorized by user goal)

- **Representative chain**: Ethereum mainnet (1) ONLY.
- **COVER**:
  1. Off-chain EIP-712 `Order` signature (domain `name="Gnosis Protocol"`, `version="v2"`, verifyingContract=GPv2Settlement `0x9008d19f…ab41`, primaryType=`Order`) → `amm sign_intent_order` (venue=`cow_swap`). The dominant CowSwap user action.
  2. On-chain `GPv2Settlement.invalidateOrder(bytes orderUid)` (`0x15337bc0`) → `amm cancel_intent_order` (venue=`cow_swap`). Owner-checked user cancellation; cancel semantics are terms-free so an opaque orderUid suffices.
- **DEFER** (user-facing, each with a P2-measured 1st-party usage-share — NOT prose):
  - `GPv2Settlement.setPreSignature(bytes,bool)` (`0xec6cb13f`): on-chain pre-signature of an **opaque orderUid** (56B digest + 20B owner + 4B validTo); calldata carries NO sellToken/buyToken/amount, so static decode yields no swap intent to scope — the economic terms are already analyzed at off-chain `Order` signing time. SC-wallet-dominant minor path.
  - `CoWSwapEthFlow.createOrder` (prod `0xba3c…adec`): native-ETH order wrapper (payable); ETH-seller-only secondary surface. createOrder calldata *does* carry order terms, so coverable in a follow-up slice.
  - `ComposableCoW.create / setRoot`: conditional/programmatic orders (TWAP etc.), predominantly Safe smart wallets via ExtensibleFallbackHandler.
- **EXCLUDE** (definitionally not user-facing):
  - `settle` / `swap` — solver-only (onlySolver, allowlisted).
  - `freeFilledAmountStorage` / `freePreSignatureStorage` — onlyInteraction (mid-settlement, Settlement-internal).
  - `simulateDelegatecall` / `simulateDelegatecallInternal` — simulation helpers.
  - GPv2AllowListAuthentication (proxy+impl) — governance (solver allowlist).
  - GPv2VaultRelayer — ERC-20 allowance target (analyzed by the tokens:erc20 standard adapter, not a CoW-specific surface).
  - CoWSwapEthFlow barn `0x04501b…6e95` — staging.
- **SCOPE ORACLE wrinkle (signature-surface)**: CowSwap's primary surface is OFF-CHAIN — the user only *signs* an `Order`, they do not submit a top-level tx (the solver does, via `settle`). So coverage-share CANNOT be measured by `tx.to`. It is measured at the **order/trade level** via Dune `cow_protocol_ethereum.*` (orders/trades) or the CoW orderbook API: what fraction of CoW orders are plain EIP-712/eth_sign `Order` signatures (covered) vs presign / ethflow / ComposableCoW (deferred). Recorded in the P2 SCOPE ORACLE row.

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | Scope Classification + SCOPE CONTRACT above: mainnet(1) only, wallet-facing (off-chain Order sig + on-chain cancel), multichain deferred. |
| Codex current-session research executed | done | Main session (Claude Opus 4.8) WebFetch'd 1st-party Solidity: GPv2Order.sol (Order struct + ORDER_TYPE_HASH 0xd5a25ba2…e489 + kind/balance consts) and GPv2Signing.sol (domain name="Gnosis Protocol", version="v2", verifyingContract=address(this); schemes EIP712/EthSign/EIP1271/PreSign). |
| Claude Code or sub-agent research executed | done | sub-agent (general-purpose, id a2a09e6ae3f13598c) produced the mainnet contract inventory from docs.cow.fi + cowprotocol/contracts networks.json + ethflowcontract/composable-cow artifacts. |
| Claude/sub-agent exact prompt or command recorded | done | Prompt embedded in plan `~/.claude-web3/plans/cowswap-onboarding.md`; mainnet-only, 1st-party-only, contract inventory + per-contract wallet-facing funcs + Order/domain. |
| Codex-only candidates listed | done | Main-session-verified: GPv2Settlement addr + Order/domain (cross-confirmed by sub-agent — no divergence). |
| Claude/sub-agent-only candidates listed | done | sub-agent surfaced beyond the core: ComposableCoW + 5 order-type handlers + CurrentBlockTimestampFactory + ExtensibleFallbackHandler + CoWUidGenerator + ethflow barn (staging). All excluded in this slice. |
| dropped-unverified candidates listed with reason | done | None dropped. Flagged unverified: EIP-55 checksum casing of the 6 ComposableCoW handler addresses (lowercase in networks.json) — recorded lowercase (gate-safe, 1st-party form); non-critical as all `exclude`. sub-agent's invalidateOrder selector guess (0x18d740b6) was WRONG → main recomputed via cast+viem = 0x15337bc0. |
| final contract inventory verified against first-party sources | done | All 15 contracts tied to docs.cow.fi + cowprotocol/{contracts,ethflowcontract,composable-cow} networks.json/broadcast. GPv2Settlement ABI fetched from Etherscan v2 getabi (chainid=1, status=1/OK), 8 external-mutating funcs. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | not applicable — CowSwap is a settlement singleton (one GPv2Settlement), not factory/pool/vault-heavy. No child universe. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | not applicable (see above). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | not applicable (no child universe). |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | not applicable (no child universe). |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | not applicable (no factory children). |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable (not pool/factory/vault-heavy). |
| token-surface inventory completed or explicitly scoped out | done | scoped out — CowSwap trades arbitrary user-supplied ERC-20s; the venue issues NO LP/share/receipt/debt token. sell/buy tokens are analyzed by the tokens:erc20 standard adapter. (The COW governance token is unrelated to the swap surface.) |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | surface/cowswap/_deployments.json — 15 mainnet contracts (1 cover GPv2Settlement, 14 exclude with reasons). |
| `npm run check:surface` output recorded | done | I1 PASS (GPv2Settlement: 8 surface · 1 cover · 7 exclude). Remaining: I2 invalidateOrder + S2 signed_structs.Order have NO manifest — EXPECTED at P0, resolved in P1. Full output /tmp/cowsurf.txt. |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | (a) off-chain Order sig (typed-data, selector placeholder 0x0000000d) → amm sign_intent_order (venue=cow_swap, order_kind=limit). (b) invalidateOrder 0x15337bc0 → amm cancel_intent_order (venue=cow_swap). Both reuse EXISTING amm Tier-3 (no new Rust). |
| permission/fund-movement/red-flag selector review recorded | done | Order signature IS the fund-movement primitive (authorizes selling sellAmount of sellToken for ≥buyAmount of buyToken) — analyzed by sign_intent_order scope (sell/buy/amount/recipient decoded). invalidateOrder = cancellation (no fund movement). setPreSignature DEFERRED (opaque orderUid carries no terms — see SCOPE CONTRACT). No protocol-specific permission-grant in scope (VaultRelayer ERC-20 approve = tokens:erc20 standard adapter). |
| manifest files added/changed listed | done | registryV2/manifests/cowswap/order/sign@1.0.0.json (typed-data Order), registryV2/manifests/cowswap/settlement/invalidate-order@1.0.0.json (on-chain cancel). |
| enrichment/live_field decision recorded for every COVER action | done | sign_intent_order: live_inputs expected_fill_price + competing_orders = venue_api (CoW orderbook API quote/orders endpoints), declared but DORMANT (mirror of UniswapX; no configured endpoint — enrichment is the deferred pattern, verdict is 100% local WASM Cedar). cancel_intent_order: no live_inputs (terms-free). |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | venue_api enrichment is dormant (no local handler / no configured endpoint), identical to the shipped UniswapX sign manifests; verdict path is fully local (no live RPC call). Not a blocker. |
| Tier3 not needed or full Tier3 downstream contract completed | done | NOT needed — amm::SignIntentOrder + IntentVenue::CowSwap{chain,settlement} + amm::CancelIntentOrder already exist with lowering (lowering_v2/amm/sign_intent_order.rs, cancel_intent_order.rs) and a conformance test (sign_intent_venue_cow_swap_conforms, GPv2Settlement addr pinned). Re-verified the field mapping against 1st-party GPv2Order.sol/GPv2Signing.sol (♻️ re-entry). |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | n/a — no new Tier3. Reused: action/src/amm/intent.rs (SignIntentOrderAction/CancelIntentOrderAction/IntentVenue::CowSwap), transition/src/effect/amm/intent_order.rs, lowering_v2/amm/{sign,cancel}_intent_order.rs, action/src/view.rs. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- validate --filter cowswap` = "1 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24]" — the on-chain invalidate-order manifest decodes cleanly to amm cancel_intent_order. The typed-data sign manifest is calldata-less, so it is validated via P2 corpus `--require-expect-body` (route_typed_data), not calldata validate. check:surface PASS (GPv2Settlement 1 cover · 1 on-chain manifest · 1 signed-struct). |

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

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| (none so far) | | |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is `done` or has a concrete, user-visible `blocked` disposition and this command passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <protocol> --phase all
```
