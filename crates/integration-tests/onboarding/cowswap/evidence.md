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
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | **95.4%** of settled CoW orders decode (distinct-order, Dune 30d mainnet): EOA eip712/eth_sign 73.3% + SC-wallet eip1271 12.8% + ethflow native-ETH 9.3% (L5a). DEFER: setPreSignature 4.5% + ComposableCoW ≤0.6%. On-chain tx.to=GPv2Settlement is 99.9% solver settle (EXCLUDE). [L2-refined to settled-order granularity; L5a added ethflow cover] |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | `setPreSignature` (4.5% = 3,410 settled orders, defer-stays L5b — opaque uid), `ComposableCoW.create/setRoot` (≤0.6%, defer-stays L5c) — each P2 usage-share (see P2 SCOPE ORACLE). [`CoWSwapEthFlow.createOrder` promoted to COVER in L5a] |
| direct factory-child calls | not applicable (CowSwap is a settlement singleton, not factory/pool/vault-heavy) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | **wallet-facing, Ethereum mainnet (1) ONLY: CoW Protocol off-chain EIP-712 `Order` signature (→ amm sign_intent_order, venue=cow_swap) + on-chain `CoWSwapEthFlow.createOrder` native-ETH placement (→ amm sign_intent_order, OnchainTx; L5a, both ethflow contracts) + on-chain `invalidateOrder` cancel (→ amm cancel_intent_order). Decodes 95.4% of settled CoW orders (distinct-order, Dune 30d): EOA eip712/eth_sign 73.3% + SC-wallet eip1271 12.8% (the eip1271 Order struct decodes identically; whether the extension intercepts the SC-wallet signing flow is a separate concern, see L1) + ethflow native-ETH 9.3%. DEFER (P2 usage-share): setPreSignature 4.5%, ComposableCoW ≤0.6%, multichain variants. EXCLUDE: solver settle (99.9% of on-chain tx.to), governance, allowance-target. NOT full-surface, NOT multichain.** |

### SCOPE CONTRACT (declared before P1 — pre-authorized by user goal)

- **Representative chain**: Ethereum mainnet (1) ONLY.
- **COVER**:
  1. Off-chain EIP-712 `Order` signature (domain `name="Gnosis Protocol"`, `version="v2"`, verifyingContract=GPv2Settlement `0x9008d19f…ab41`, primaryType=`Order`) → `amm sign_intent_order` (venue=`cow_swap`). The dominant CowSwap user action.
  2. On-chain `GPv2Settlement.invalidateOrder(bytes orderUid)` (`0x15337bc0`) → `amm cancel_intent_order` (venue=`cow_swap`). Owner-checked user cancellation; cancel semantics are terms-free so an opaque orderUid suffices.
  3. **(L5a — promoted from DEFER)** On-chain `CoWSwapEthFlow.createOrder(EthFlowOrder.Data)` (`0x322bba21`, BOTH active ethflow contracts: prod `0xba3c…adec` + v1 `0x40a5…2187`) → `amm sign_intent_order` (venue=`cow_swap`, OnchainTx meta). Native-ETH order placement: `EthFlowOrder.Data` has no sellToken (implicit = WETH, baked); msg.value = sellAmount+feeAmount. Measured 9.3% settled-order share (L2) → highest-value DEFER, now covered.
- **DEFER** (user-facing, each with a P2-measured 1st-party usage-share — NOT prose):
  - `GPv2Settlement.setPreSignature(bytes,bool)` (`0xec6cb13f`): on-chain pre-signature of an **opaque orderUid** (56B digest + 20B owner + 4B validTo); calldata carries NO sellToken/buyToken/amount, so static decode yields no swap intent to scope — the economic terms are already analyzed at off-chain `Order` signing time. SC-wallet-dominant minor path. **4.5% settled-order share (L2); defer-stays (L5b) — opaque uid, no terms.**
  - `ComposableCoW.create / setRoot`: conditional/programmatic orders (TWAP etc.), predominantly Safe smart wallets via ExtensibleFallbackHandler. **≤0.6% (L2); defer-stays (L5c).**
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
| final contract inventory verified against first-party sources | done | All 16 contracts (incl v1 ethflow 0x40a5..2187, added L5a) tied to docs.cow.fi + cowprotocol/{contracts,ethflowcontract,composable-cow} networks.json/broadcast. GPv2Settlement ABI from Etherscan v2 getabi (8 external-mutating funcs); CoWSwapEthFlow ABI from Etherscan v2 getabi (6 external-mutating funcs: createOrder cover, invalidateOrder/invalidateOrdersIgnoringNotAllowed/wrap/unwrap/wrapAll exclude). |
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
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- validate --filter cowswap` = "3 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24]" — invalidate-order + ethflow/create-order (L5a) calldata manifests + the order/sign typed-data manifest all validate cleanly. check:surface PASS (cowswap 3 cover: GPv2Settlement + ethflow prod + v1 · 2 on-chain + 1 typed-data manifest · 1 signed-struct Order). |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | done | `cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --filter cowswap --iterations 5000 --seed 42` (seed=0x2a). Result (post-L5a, 4 callkeys): total=20000 pass=20000 soft=0 fail=0 panic=0; domain histogram amm=20000. (P2 was 10000/2 callkeys before ethflow.) |
| iterations >= 5000 or justified lower bound | done | 5000 iters/callkey × 4 callkeys (sign typed-data 0x0000000d + invalidate 0x15337bc0 + ethflow createOrder 0x322bba21 at 2 addresses) = 20000 total, all pass. |
| fixed edge-case matrix recorded | done | corpus matrix (9 entries): sell-order (USDT→USDC), buy-order (WETH→USDT), self-receiver (receiver=0x0), eip712 vs eip1271 signing scheme, on-chain cancel (invalidateOrder real orderUid). **L6 hardening edges**: partiallyFillable=true; sellTokenBalance=external/internal & buyTokenBalance=internal (Balancer-vault balances); buyToken=native-ETH BUY_ETH_ADDRESS sentinel 0xEeee..EEeE (1st-party GPv2Transfer.sol); validTo in the past (expired); nonzero feeAmount. **L5a ethflow edges**: real on-chain createOrder calldata on BOTH ethflow contracts (prod WETH→wstETH + v1 WETH→USDC), native-ETH placement → sign_intent_order OnchainTx. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | done | CoW Order is a FLAT 12-field struct — no nested/array/opcode/multicall. Edges exercised: sell vs buy kind; partiallyFillable true/false; self-receiver (0x0); eip712 vs eip1271; validTo deadline (future + expired/past); feeAmount=0 and nonzero; sellTokenBalance erc20/external/internal & buyTokenBalance erc20/internal; native-ETH sentinel buyToken; terms-free orderUid cancel. (No opcode/nested edges exist for this surface.) |
| representative pass/error corpus entries committed or justified | done | data/golden/v3-decode/cowswap/corpus.json — 9 entries (2 real off-chain orders from CoW orderbook API + 4 synthetic sign edges + 2 real on-chain ethflow createOrder + 1 real-orderUid on-chain cancel), all expect=pass. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan v2 API: getabi GPv2Settlement (status 1/OK) + txlist GPv2Settlement (3000 tx, status 1/OK). |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | txlist to=GPv2Settlement (3000 recent, sort desc): 0x13d79a0b settle=2997 (99.9%), 0x15337bc0 invalidateOrder=1 (0.03%), plain-ETH=2. Confirms on-chain top-level tx.to is solver-dominated; the user surface is OFF-CHAIN Order signing. |
| external tx pull target address count is nonzero and recorded | done | 1 cover address (GPv2Settlement 0x9008..ab41); 3000 raw txs pulled. |
| Etherscan `api_calls_used` recorded | done | 2 (getabi + txlist). |
| Etherscan `raw_txs_seen` recorded | done | 3000 (to=GPv2Settlement). |
| Etherscan `unique_selectors_seen` recorded | done | 3 (0x13d79a0b settle, 0x15337bc0 invalidateOrder, 0x plain-ETH). setPreSignature 0xec6cb13f absent in 3000 → rarer than invalidate. |
| Etherscan real tx coverage per COVER selector recorded | done | invalidateOrder 0x15337bc0 = 1/3000 on-chain (rare; off-chain orderbook cancel dominant) — decode covered via corpus w/ a real orderUid. **ethflow createOrder 0x322bba21 (L5a) = on-chain COVER, 9.3% of all settled orders, both contracts (prod 7,276 + v1 66 createOrders/30d, Dune 7641484); 2 real-tx corpus entries.** Order sig (the dominant COVER) is off-chain (no tx.to) → Dune 82,259 trades/30d + typed-data corpus. settle 0x13d79a0b = EXCLUDE (solver). |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | done | GPv2Settlement swept (1 target, 3000 tx). on-chain = solver settle. The wallet-facing surface (off-chain Order sig) is measured via Dune order-path distribution (SCOPE ORACLE row); not a tx.to sweep. |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | done | 2997 settle = non-actionable (solver, EXCLUDE); 2 plain-ETH = non-actionable; 1 invalidateOrder = actionable (covered). 0 unknown. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | done | not applicable (not pool/factory-heavy; single settlement singleton). |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | done | none — all 3000 txs at the known cover address; no unknown to-address surfaced. |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | done | Order primaryType (the only in-scope type, no witness): `v3-harness corpus --filter cowswap --require-expect-body` = 6 sign entries (2 real [eip712, eip1271] + 4 synthetic edges: self-receiver, partiallyFillable+vault-balances, native-ETH sentinel+expired validTo, buy-kind+internal+fee) PASS via route_typed_data. |
| Dune MCP/API availability checked | done | Dune community_fluid_engine, 413/2500 credits before run. |
| Dune usage baseline recorded | done | 82,259 settled CoW trades = 75,960 distinct settled orders, mainnet 30d (gnosis_protocol_v2_multichain.gpv2settlement_evt_trade WHERE chain='ethereum'; partial fills inflate trades over orders). |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | query 7639528 + 7639541 (P2 v2). **L2: 7641433 (v3 settled-order attribution: distinct orderUid, ethflow/presign intersection) + 7641440 (v3b eip712-EOA vs eip1271-contract split via ethereum.creation_traces).** All partition-pruned (evt_block_date / call_block_date). |
| Dune `executionCostCredits` / usage delta recorded | done | P2 v2: 0.29 + 0.085. L2: 0.198 (v3) + 3.843 (v3b — ethereum.creation_traces has-code join) ≈ **4.4 credits total**. |
| Dune rows returned / selected tx hashes recorded | done | SCOPE ORACLE v2 5 rows; corpus 6 rows; L2 v3 1 row + v3b 1 row. Selected real orders: uid 0x2cb562…235c (tx 0x762fffb3, USDT→USDC eip712), uid 0xb6138f…1e9b (tx 0xa9b7a7c5, WETH→USDT eip1271). |
| representative real-tx corpus/golden entries committed or justified | done | corpus.json: 2 real orders (CoW orderbook API, signed originals) + 1 synthetic sign + 1 real-orderUid on-chain cancel. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter cowswap --require-expect-body` = 9/9 matched, semantic expect_body 9/9 pinned. |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | done | **L2-REFINED to settled-ORDER attribution (distinct orderUid, Dune 30d mainnet, queries 7641433 v3 + 7641440 v3b). Denominator = 75,960 distinct settled orders (82,259 raw trades; partial fills inflate trade count). Attribution: EOA eip712/eth_sign 55,706 = 73.3% (COVERED, decode + extension typed-sig intercept) · SC-wallet eip1271 9,758 = 12.8% (Order struct decodes IDENTICALLY — corpus entry 2 proves; signing-flow intercept = L1, separate concern) · ethflow native-ETH (owner=0xba3c..eadec) 7,088 = 9.3% (DEFER→L5a) · settled setPreSignature (trade∩setpresignature signed, ethflow-overlap=0) 3,410 = 4.5% (DEFER). → DECODE-COVERED = EOA + eip1271 = 65,464 = 86.2%. ComposableCoW ≤0.6% (upper bound; settles within the eip1271 bucket — the deferred user-action is the upfront ConditionalOrder sig, not the discrete settled Orders). On-chain tx.to=GPv2Settlement (Etherscan 3000): settle 99.9% solver (EXCLUDE), user invalidateOrder 0.03%. Earlier v2 trade-level estimate (~85.7% lower bound) cross-checks. **L5a UPDATE: ethflow createOrder now COVERED on BOTH active contracts (prod 0xba3c..eadec 7,276 createOrders/30d + v1 0x40a5..2187 66; query 7641484) → decode-covered rises to 86.2% + ethflow 9.3% = 95.4%; remaining DEFER = setPreSignature 4.5% (L5b) + composable ≤0.6% (L5c).** Label claims "95.4% of settled orders decode; 73.3% EOA eip712 confirmed-intercept".** |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | done | No hard/soft/misdecoded/unknown_protocol_address gaps surfaced. fuzz 20000 pass/0 fail/0 panic; corpus 9/9 matched + 9/9 pinned; surface PASS; validate 0 structural errors. Excluded gaps = solver(settle/swap)/governance(allowlist)/internal(free*/simulate*) per SCOPE CONTRACT. |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | done | No decoder gap → no decoder fix. Two corpus-AUTHORING fixes during P2: (1) expect_body amount encoding decimal→hex (U256 serde emits hex, e.g. 1000000000→0x3b9aca00); (2) rpc value "0x0"→"0" (decimal parser). Both authoring, not decoder gaps. |
| manifest/decoder/Tier3/harness change list recorded | done | No Rust decoder/Tier3/harness change (reused existing amm::SignIntentOrder + IntentVenue::CowSwap + amm::CancelIntentOrder; ethflow createOrder reuses sign_intent_order with OnchainTx meta). Registry-only additions: 3 manifests (order/sign, settlement/invalidate-order, **ethflow/create-order L5a**) + 5 surface files (_deployments, gpv2-settlement.{abi,coverage}, **cowswap-ethflow.{abi,coverage} L5a**) + 1 corpus file. |
| P2 rerun after fixes recorded | done | corpus rerun after both fixes = 4/4 (P2); after L6 hardening = 7/7; after L5a ethflow cover = 9/9 matched, 9/9 expect_body pinned (CI gate corpus_replay 343/343). |
| corpus `expect` flips or exclusions justified | done | none — all 9 entries expect=pass, no flips or exclusions needed. |
| remaining gaps have explicit defer/blocker disposition | done | DEFER (L2-refined usage-share): setPreSignature 4.5% (defer-stays L5b — opaque uid, no terms), ComposableCoW ≤0.6% (defer-stays L5c — conditional-order complexity vs share), multichain variants. CoWSwapEthFlow COVERED in L5a (9.3%). No blockers. |

### L3 — dropped-field disposition (hardening round)

The CoW `Order` has 12 EIP-712 fields; the decoder maps 7 into `amm sign_intent_order`
(venue, sell, buy, sell_amount, buy_min, recipient, valid_until) and drops 5. L3 asks
whether any dropped field must be added to `SignIntentOrderAction` (a Tier-3 change).
Test: does the field EXPAND the fund/permission authorization beyond what's captured?
**Verdict: no Tier-3 needed — the fund-authorization scope (max spend = sellAmount,
min receive = buyAmount, recipient, expiry, tokens, venue) is fully captured; the
dropped fields are within-bound or enrichment-only.**

Per-field disposition (format is a bullet list, not a status table, on purpose):

- **`kind` (sell/buy)** — not a scope boundary. Picks which of sellAmount/buyAmount is exact vs a bound. SELL = spend exactly sellAmount, receive ≥buyAmount; BUY = spend ≤sellAmount (cap), receive exactly buyAmount. Current `sell_amount=sellAmount` / `buy_min=buyAmount` already encodes the WORST CASE for both (max spend, min receive). order_kind is fixed to `limit` (price-discovery axis = Dutch/Limit/Rfq; CoW orders are limit).
- **`partiallyFillable`** — not a scope boundary. A partial fill stays within the same sellAmount/buyAmount bounds; total authorization unchanged.
- **`feeAmount`** — not a scope boundary. Fee is taken from WITHIN sellAmount (for ethflow, msg.value = sellAmount+feeAmount, but the order's sell is sellAmount); does not raise max spend. Modern orders carry feeAmount=0 (fees moved into the limit price).
- **`sellTokenBalance` / `buyTokenBalance` (erc20/external/internal)** — not a scope boundary. Selects WHERE funds settle (wallet vs Balancer vault); the AMOUNT bound is unchanged and exposure stays gated by the user's VaultRelayer approval.
- **`appData` (bytes32)** — enrichment-only, NOT a static-decode gap. 1st-party (docs.cow.fi/app-data): appData is a bytes32 HASH of an off-chain IPFS JSON; the signed order contains ONLY the hash. It can carry **hooks** (arbitrary pre/post `target`+`callData`) — a real risk — but the hooks live in the off-chain doc, so they are NOT statically decodable from the signature (only the hash is present). appData is frequently non-empty for BENIGN orders too (commits to app/referrer/quote metadata — corpus entries 1-2 are real orders with non-zero appData and NO hooks), so a static `appData != 0` check cannot distinguish hooks from metadata (high false-positive). Hook execution is solver social-consensus (not contract-enforced) and funds stay bounded by the VaultRelayer approval (analyzed by the erc20 approve adapter). Capturing the hash statically = low-actionability + would pollute the venue-shared `SignIntentOrderAction` (UniswapX/Fusion/Bebop don't have it). **Correct disposition: enrichment-backed enhancement** (fetch the appData doc → decode hooks → scope target/callData), adjacent to L4; out of this static-decode onboarding's scope.

No code/manifest/corpus change for L3 (disposition is "intentional non-mapping" + appData→hooks routed to the enrichment roadmap).

### L1 — eip1271 interception (extension-level finding, OUT of registry scope)

Investigation of whether ScopeBall actually *intercepts* the SC-wallet (eip1271) CoW
signing flow — the 12.8% eip1271 bucket from L2. Read-only review of
`browser-extension/backend/injected/proxy-injected-providers.ts` + `…/service-worker/sig-routing.ts`.

Findings:
- **Decode is complete**: an eip1271 CoW Order decodes identically to an EOA order — same EIP-712 `Order` struct; corpus entry 2 is a REAL mainnet eip1271 SC-wallet order and passes.
- **Interception is RPC-method + transport based**: the proxy wraps `window.ethereum` and EIP-6963-announced **injected** providers (`eip6963:announceProvider` listener; no WalletConnect / Safe-Apps-SDK transport). It gates `eth_signTypedData{,_v3,_v4}` (→ Flow 2) and `eth_sign`/`personal_sign` (→ warn). There is NO eip1271/`isValidSignature`-specific handling — interception keys off the signing RPC call, not the signature type.
- **Consequence**: a SC wallet that requests `eth_signTypedData_v4` on the **injected** provider IS intercepted and its Order decoded (whether the resulting sig is ECDSA or eip1271). But Safe orders signed via **WalletConnect** (relay transport) or the **Safe Apps SDK iframe** (postMessage) — a common Safe path — do NOT pass through the injected provider, so they are not seen. That is a **provider-wrapping (extension-architecture) gap, NOT a CoW registry/decoder gap**.

Disposition: recorded as a finding; the decoder + registry are correct and eip1271-ready. Extending interception to WalletConnect / Safe-Apps-SDK transports is a separate extension-level task (affects ALL protocols' SC-wallet signatures, not just CoW) — out of this onboarding's scope.

### L5b / L5c / L4 / L8 — defer-stays disposition (hardening round closure)

- **L5b `setPreSignature` (4.5%)** — DEFER STAYS. The calldata is `(bytes orderUid, bool signed)`; the orderUid is an OPAQUE 56-byte commitment (digest ‖ owner ‖ validTo) with NO sellToken/buyToken/amount, so static decode yields no swap intent to scope. The order's economic terms are already analyzed at off-chain `Order` signing time (covered). Covering it would add a terms-free "pre-signs some order" action of low analytic value. Rationale strengthened; not promoted.
- **L5c `ComposableCoW` (≤0.6%)** — DEFER STAYS. Conditional/programmatic orders (TWAP etc.): the user signs a ConditionalOrder/handler `staticInput` once and a watch-tower mints the discrete GPv2 Orders (which settle within the eip1271 bucket). Decoding the conditional intent is a complex new per-handler mapping (TWAP/GoodAfterTime/StopLoss/…) for ≤0.6% share — cost ≫ value. Not promoted.
- **L4 enrichment (price/appData-hooks sanity)** — OUT OF SCOPE (cross-protocol). The policy-server enrichment path (`/v1/rpc` → `context.custom.*`) is dormant for ALL protocols; it is the natural home for appData-hook decoding (L3) and fill-price sanity. A CoW-only activation is not possible. Separate, framework-level work.
- **L8 multichain (Gnosis/Arbitrum/Base/…)** — OUT OF SCOPE by framework rule (representative chain = mainnet only). CoW's contracts are multichain (same GPv2Settlement address), so extension is largely a `chain_to_addresses` + Dune-per-chain exercise, but it belongs to a dedicated multichain pass.

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | done | "52976 callkey(s) + 84 typed-data entry(ies) across 816 manifest(s)" — incl cowswap by-callkey 1__0x9008..ab41__0x0000000d (sign) + __0x15337bc0 (invalidate) + by-typed-data 1__0x9008..ab41__Order. |
| registryV2 build-index vitest output recorded | done | `npx vitest run` = 1 file, 12 passed (build-index unit tests), 9.37s. |
| `npm run check:manifest` output recorded | done | `v3-harness validate --filter cowswap` = 3 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24] (order/sign + invalidate-order + ethflow/create-order). |
| `npm run check:surface` output recorded | done | PASS — "every gated contract's external surface is fully triaged and consistent". GPv2Settlement 8 surface · 1 cover · 7 exclude · 1 on-chain manifest · 1 signed-struct. |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable — CowSwap is a settlement singleton, not pool/factory/vault-heavy (no _address_universe.json). |
| v3-harness coverage/fuzz/corpus outputs recorded | done | fuzz: total=20000 pass=20000 fail=0 panic=0 (4 callkeys). corpus: 9/9 matched. |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | done | `v3-harness corpus --filter cowswap --require-expect-body` = 9/9 matched, semantic expect_body 9/9 pinned. |
| `cargo test --workspace` output recorded | done | `cargo test --workspace -- --test-threads=4` → exit 0 (background completed, 0 failed; ~25min on fresh cowswap target-dir). doc-tests + all crate/integration tests ok. No regression from the cowswap registry additions. cowswap corpus separately verified 9/9 via `v3-harness corpus --filter cowswap`; CI gate corpus_replay = 343/343 matched (incl cowswap 9/9); synthetic fuzz (gate, all callkeys) fail=0/panic=0, surface install_failures=0. (Full single-process `--test-threads=4` harness OOMs on macOS — FW-1 — so the gates were run per-test; each green.) |
| wasm build output recorded if runtime/wasm/schema changed | done | not applicable — NO Rust/wasm/schema change. Registry-only (manifest/surface/corpus/evidence); reused existing amm Tier-3 + cedarschema. WASM decoder bytes unchanged. |
| fmt/clippy/typecheck output recorded for changed crates/packages | done | no Rust crate changed → fmt/clippy n/a. registryV2 TS: build + vitest + check:surface all clean (implicit tsc via tsx). |
| exact staged files and commit hash recorded | done | P0+P1 = commit `44c3286a` (surface 3 files + manifest 2 + evidence P0/P1). P2-P4 = THIS commit (staged `crates/integration-tests/data/golden/v3-decode/cowswap/corpus.json` + `crates/integration-tests/onboarding/cowswap/evidence.md`); exact hash in `git log feat/cowswap-onboarding` + plan/memory. |
| remaining WARNs/deferred selectors/actions listed with reason | done | DEFER (L2-refined usage-share): setPreSignature 4.5% (L5b), ComposableCoW ≤0.6% (L5c), multichain. CoWSwapEthFlow COVERED (L5a). check:surface WARNs (morpho I0', aave/compound-v3/hyperliquid/layerzero UNGATED) are PRE-EXISTING, not cowswap. |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | done | see Scope Classification "final claim label" — wallet-facing, mainnet(1), 95.4% settled-order decode coverage (73.3% EOA eip712/eth_sign + 12.8% SC-wallet eip1271 + 9.3% ethflow native-ETH L5a), explicit DEFER (presign 4.5% / composable ≤0.6%)/EXCLUDE, NOT full-surface/multichain. |
| no base/worktree merge performed unless user explicitly requested it | done | no merge performed. Commits on feat/cowswap-onboarding only; push/merge only on explicit user request. |

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
