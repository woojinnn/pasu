# Audit Report — Phase 13 Curve declarative-adapter

**Audit date**: 2026-05-21
**Auditor**: solidity-auditor sub-agent (read-only)
**Scope**: Phase 13 Curve changeset (Router NG multi-chain, P1-1/P1-5/P1-6 fixes, Tier B B1/B2/B3) + the 65 post-Phase-12 unaudited bundles
**Worktree**: `.claude/worktrees/phase13-curve` (branch `worktree-phase13-curve`)
**Baseline**: `cargo test --workspace` = **840 passed / 0 failed / 6 ignored** (measured this run)
**Reference**: `AUDIT-PHASE12-CURVE.md` (P0-1/P0-2 FIXED), `docs/PHASE13-CURVE-RESEARCH.md` (Phase-1 1st-source research)

---

## Executive Summary

ScopeBall's threat model: a declarative adapter bundle that mis-describes a transaction = a user
Cedar policy silently bypassed. The Phase 13 changeset is small (38 files, +261/-114) and well-targeted.
The four headline changes (A.1 Router NG multi-chain, A.2 P1-1 `debtToCover` re-map, A.3 P1-6 frxETH LP
address, A.4 Tier B B1/B2/B3) are all **substantively correct in their core logic**. B2 — the highest-risk
change, a live BuiltinFn rewrite driving swap output-token inference — was verified line-by-line against
`Router.vy::exchange` and is **correct, including the fail-closed paths**.

However the audit surfaced **one P1** that the Phase-13 work itself introduced (the Router NG multi-chain
extension relies on a cartesian `chain_ids × to` index expansion that produces 52 spurious callkeys, some
of which land on real deployed contracts), **one P1 residual** (the P1-6 fix is partial — it missed
`frxeth/removeLiquidityImbalance-2`), and several P2/P3 envelope-semantic gaps in the 65 newly-audited
bundles that mirror the carry-forward P1-2/P1-3 class.

**Findings**:
- **0 P0** — no direct policy bypass / fund-loss-enabling mis-decode introduced.
- **2 P1** — cartesian callkey topology (spurious-contract mis-decode); partial P1-6 fix.
- **4 P2** — `borrow_more`/`create_loan_extended` collateral lever lost; gauge `withdraw` amount lost;
  `liquidate_extended` `frac`/`callbacker` lost; crvUSD `repay`/`liquidate` `use_eth` ignored.
- **3 P3** — `repayExtended` amount=unknown observability; `host:token_metadata` over-claim; 4x5 per-hop
  swap_type semantics unconfirmed (carried from research doc §3).

**P0 count: 0 — phase gate PASS.**

---

## Phase-13 changeset verdict (A.1 – A.4)

| Item | Verdict | Note |
|---|---|---|
| **A.1** Router NG multi-chain (`exchange@1.0.0` → 10 chains; new `exchange-legacy@1.0.0` 4 chains) | **PASS w/ P1-1** | selector/ABI/chain-split all correct; 4-arg `recipient`←`$.tx.from` correct. But the index/bridge cartesian expansion is a topology hazard — see P1-1. |
| **A.2** P1-1 `seizedCollateralAmount` → `debtToCover` (12 liquidate + selfLiquidate) | **PASS** | Semantically correct. Curve `liquidate(_user, min_x)` `min_x` = minimum *debt asset* (crvUSD) received → `debtToCover` (kind=min). `seized_collateral_amount` correctly left unset. mapper.rs test updated. |
| **A.3** P1-6 frxETH LP address (`0xf432...6C7A` frxETHCRV) | **PASS w/ P1-2 residual** | LP address verified on-chain (`symbol()`=`frxETHCRV`, `minter()`=pool `0xa1F8…E577`). But only 3 of the 4 frxETH liquidity bundles fixed — `removeLiquidityImbalance-2` still missing. See P1-2. |
| **A.4** Tier B B1 (curve.rs +Kava/+X-Layer), B2 (`curve_route_last_token` rewrite), B3 (`enum_tagged` wire) | **PASS** | B1 addresses verified vs README @ `1014d369`. B2 early-break logic verified equivalent to `Router.vy::exchange` `for i in range(5)` + break on `_route[i*2+3]==empty` — including fail-closed on zero `route[1]` / zero output slot. B3 dispatcher correctly resolves `CURVE_ROUTER_NG_SWAP_TYPES`. |

---

## P0 — Critical

**None.** No Phase-13 change introduces a direct Cedar-policy bypass or a fund-loss-enabling mis-decode.

---

## P1 — High

### P1-1: Router NG multi-chain bundle relies on a cartesian `chain_ids × to` callkey expansion that mis-pairs chains and addresses, producing 52 spurious callkeys that match real-but-unrelated deployed contracts

**Severity**: P1 (High) — conditional mis-decode of an arbitrary contract as a Curve Router NG swap.

**Files**:
- `registry/manifests/curve/router-ng/exchange@1.0.0.json:6-14` (`chain_ids:[1,10,56,100,137,250,8453,42161,43114,2222]` × `to:[5 addresses]`)
- `registry/manifests/curve/router-ng/exchange-legacy@1.0.0.json:6-13` (`chain_ids:[252,5000,196,324]` × `to:[4 addresses]`)
- `registry/scripts/build-index.ts:190,199-214` (cartesian product: `callkeyCount = chain_ids.length * to.length`)
- `crates/policy-engine-wasm/src/declarative_exports.rs:99-113` (`register_bridge_entries` — identical cartesian expansion of the WASM bridge table)

**Description**:

Curve Router NG is deployed at a **different address per chain** (`docs/PHASE13-CURVE-RESEARCH.md` §1.1). The
correct chain→address relation is a 14-element *mapping*, not a product:

```
chain 1     → 0x45312ea0…cd4e        chain 252   → 0x56C526b0…0cD7
chain 10    → 0x0DCDED35…d983        chain 5000  → 0x4f37A9d1…fc1F
chain 56    → 0xA72C85C2…51CC        chain 196   → 0xBFab8ebc…1855
chain 100   → 0x0DCDED35…d983        chain 324   → 0x7C915390…27FA
chain 137   → 0x0DCDED35…d983
chain 250   → 0x0DCDED35…d983
chain 8453  → 0x4f37A9d1…fc1F
chain 42161 → 0x2191718C…0A0D
chain 43114 → 0x0DCDED35…d983
chain 2222  → 0x0DCDED35…d983
```

The Phase-13 bundle expresses this as `match.chain_ids[]` + `match.to[]`, and **`build-index.ts:199-200`
plus `register_bridge_entries` (declarative_exports.rs:103-104) both take the full `chain_ids × to`
cartesian product**. For `exchange@1.0.0` that is 10 chains × 5 addresses = **50 callkeys, of which only
10 correspond to a real `(chain, address)` Router NG deployment**. `exchange-legacy@1.0.0` adds 4 × 4 = 16
callkeys, 4 real. **Total: 66 callkeys generated, 14 real → 52 spurious.**

There is **no chain↔address pairing re-verification** anywhere downstream. The registry index
(`index/by-callkey/<chain>__<to>__<sel>.json`), the JIT fetcher, and the WASM bridge all key purely on
`(chain_id, to_lower, selector_lower)` exact-match. A spurious callkey is indistinguishable from a real one.

A spurious callkey is only *harmless* if no contract is deployed at that `(chain, address)`. On-chain
probing shows that **is not the case**:

- `0x0DCDED3545D565bA3B19E683431381007245d983` — the canonical Router NG on chains 10/100/137/250/2222 —
  is **also a deployed contract on Ethereum mainnet (chain 1)**: 41,541-hex bytecode, a Vyper 0.3.7
  contract (trailer `a165767970657283000307000b` = `vyper 0.3.7`), codehash
  `0x3486…71e5` ≠ the canonical Optimism Router NG codehash `0x12ae…4825`. The bundle generates the
  spurious callkey `1__0x0dcded3545d565ba3b19e683431381007245d983__0xc872a3c5`.
- `0x4f37A9d177470499A2dD084621020b023fcffc1F` — the canonical Router NG on chains 8453/5000 —
  is **also a deployed Vyper contract on Ethereum mainnet (`0x6004361015…`) and on Arbitrum
  (`0x6004361015…`)**, with a bytecode prefix that differs from the canonical Base router. The
  `exchange@1.0.0` bundle generates spurious callkeys for it on chains 1/10/56/100/137/250/42161/43114/2222;
  `exchange-legacy@1.0.0` generates spurious callkeys for it on Fraxtal 252 (it is a deployed contract there too).

**Risk**: A dApp issues a transaction to one of the contracts sitting at a spurious `(chain, address)`
pair with the 4-byte prefix `0xc872a3c5` (5x5) or `0xaad348a2` (4x5). ScopeBall's declarative path matches
the Curve Router NG bundle, ABI-decodes the calldata as `exchange(address[11], uint256[5][5], …)`, and
emits a `SwapAction` envelope asserting an input token (`_route[0]`), an output token
(`curve_route_last_token(_route)`), and amounts. The Cedar policy is then evaluated against a swap that
the target contract does **not** actually perform. This is exactly the P0-2 class from the Phase 12 audit
("calldata interpretation diverges from on-chain semantics → policy gate bypassed") — re-introduced at a
*different layer* (the index topology rather than a hardcoded literal).

Two mitigating facts keep this at P1 rather than P0:
1. The specific contracts probed at the spurious pairs (`0x0DCDED…` on mainnet, and the inert Vyper
   contracts) **revert on all external calls including bogus selectors** — so a `0xc872a3c5` tx to them
   would revert on-chain, moving no value. A reverting tx that the user signs anyway costs only gas.
2. The selectors `0xc872a3c5` / `0xaad348a2` are *Curve-specific* (`exchange(address[11],uint256[N][5],…)`)
   — the probability that a spurious-pair contract exposes a *different, value-moving* function under the
   same 4-byte prefix is low but **not verified to be zero**. The audit could not enumerate every contract
   at all 52 spurious pairs; the analyzer's correctness should not depend on that enumeration.

This is a **systemic** issue, not unique to Router NG: every multi-chain bundle with `len(chain_ids) > 1`
and `len(to) > 1` where the address is not chain-invariant has the same hazard. Router NG is the first
Curve bundle to hit it (crvUSD/gauge/stableswap bundles are all single-chain or single-address).

**Remediation**:
1. **Preferred** — change the bundle `match` schema from `{ chain_ids[], to[] }` (product) to an explicit
   `deployments: [{ chain_id, to }]` list (mapping). `build-index.ts` and `register_bridge_entries`
   iterate the list directly — 14 callkeys, 0 spurious. This is a schema change (`bundle-schema.ts`,
   `types.rs`, `declarative_exports.rs`, `build-index.ts`) but it permanently closes the class.
2. **Stop-gap (no schema change)** — split Router NG into per-chain (or per-shared-address) bundles so
   each bundle's `chain_ids × to` product is fully populated by real deployments. E.g. one bundle for the
   four chains that share `0x0DCDED…` (10/100/137/250/2222 → wait, that is 5 — still a product of 5×1, OK),
   one for chain 1 (`0x45312ea0`), one for 56, one for 8453, one for 42161, one for 43114. Each is
   `chain_ids[k] × to[1]` and therefore spurious-free. Verbose but correct under the current schema.
3. **Regardless of 1/2** — `build-index.ts` should warn (or fail) when `len(chain_ids) > 1 && len(to) > 1`,
   since that shape is almost always a mistake unless the address is a chain-invariant CREATE2 singleton.

---

### P1-2: P1-6 fix is partial — `stableswap/frxeth/removeLiquidityImbalance-2@1.0.0.json` still omits `inputLp.asset.address`

**Severity**: P1 (High) — same root cause and risk as the original P1-6, which the Phase-13 round set out to fix.

**Files**:
- `registry/manifests/curve/stableswap/frxeth/removeLiquidityImbalance-2@1.0.0.json:28` (`inputLp.asset.kind` = `erc20`, no `inputLp.asset.address`)
- `crates/adapters/mappers/src/declarative/single_emit.rs` (`read_asset_inline` accepts `address: None` silently — unchanged this round)

**Description**:

Phase-13 item A.3 (the P1-6 fix) added `inputLp.asset.address` / `outputLp.asset.address` =
`0xf432…6C7A` (frxETHCRV) to **three** frxETH bundles: `addLiquidity-2`, `removeLiquidity-2`,
`removeLiquidityOneCoin`. The frxETH pool has **four** liquidity bundles. The fourth —
`removeLiquidityImbalance-2@1.0.0.json` — was missed: it still declares `inputLp.asset.kind = "erc20"`
with no `address`.

This is verifiable by direct comparison: every other stableswap pool's liquidity bundle set carries the LP
address — 3pool 4/4 (`0x6c3f…E490`), steth 4/4 (`0x0632…f14E`) — and frxETH is now 3/4. The imbalance
variant is the lone gap in the whole `registry/manifests/curve/stableswap/` tree.

**Risk**: A Cedar policy of the form "only burn LP tokens of pool X" / "deny burning unknown LP tokens"
cannot match a frxETH `remove_liquidity_imbalance` call — `inputLp.asset.address` is `null`. The envelope
under-describes the transaction; the policy gate is bypassed for that one entrypoint. Identical to the
original P1-6 risk, just for the overload the fix did not reach.

**Remediation**: Add to `removeLiquidityImbalance-2@1.0.0.json` (line 28-29):
```json
"inputLp.asset.kind":    { "literal": "erc20" },
"inputLp.asset.address": { "literal": "0xf43211935c781d5ca1a41d2041f397b8a7366c7a" },
```
Then `cd registry && npm run build` to regenerate the callkey index. Separately, harden
`read_asset_inline` to reject `AssetKind::Erc20` with `address: None` (`MissingField`) — this would have
turned the gap into a build/test failure instead of a silent under-description (this hardening was
recommended in P1-6 and is still not applied).

---

## P2 — Medium

### P2-1: crvUSD `borrow_more` and `create_loan_extended` discard the user-controlled `collateral` deposit (carry-forward P1-2 extended to overloads)

**Files**: `registry/manifests/curve/crvusd/{wsteth,sfrxeth,wbtc}/borrowMore@1.0.0.json`,
`.../createLoanExtended@1.0.0.json` (6 bundles).

**Description**: `borrow_more(uint256 collateral, uint256 debt)` and
`create_loan_extended(uint256 collateral, uint256 debt, uint256 N, address callbacker, uint256[] callback_args)`
both deposit `collateral` (wstETH/sfrxETH/WBTC, from the user's wallet) **and** move debt in one atomic
call. The bundles emit a single `BorrowAction` carrying `amount = $.args.debt` only. The `collateral`
argument is never represented in the envelope; `N` and `callbacker` are also dropped.

This is the carry-forward **P1-2** class ("`create_loan` collateral lost"), which the Phase-12 audit
scoped only to the 3-arg `createLoan` bundles. The 65-bundle expansion added the `borrowMore` and
`createLoanExtended` overloads with the *same* defect — so the carry-forward P1-2 must be understood to
cover **9 bundles** (3 `createLoan` + 3 `borrowMore` + 3 `createLoanExtended`), not 3.

**Risk**: A Cedar policy "max wstETH collateral deposited per tx" cannot fire on a `borrow_more` /
`create_loan_extended` call — the field does not exist in the envelope. Borrowing against a suicidal LTV
is indistinguishable from a safe one. `createLoanExtended`'s `callbacker` (the leverage-callback address —
a token-drain surface) is fully invisible.

**Remediation**: same as carry-forward P1-2 — emit a two-envelope atomic intent
(`Borrow{crvUSD, debt}` + `Supply{collateral_token, collateral}`). Needs the policy-engine multi-envelope
plumbing; user-approved for a follow-up phase. Tracked under carry-forward.

---

### P2-2: Gauge `withdraw(uint256 _value)` discards the user-controlled unstake amount

**Files**: `registry/manifests/curve/gauge/{3pool,steth,frxeth}/withdraw@1.0.0.json` (3 bundles).

**Description**: The new gauge `withdraw` bundles map `withdraw(uint256 _value)` to a
`ClaimUnstakeAction` with `tokenOut` set (the LP token) but **`amountOut` omitted**.
`build_claim_unstake_envelope` (`single_emit.rs:934-952`) reads `amountOut` via `read_amount_inline`,
which tolerates absence. The on-chain `_value` (number of gauge LP tokens to unstake) is user-controlled
and is the natural quantity a withdrawal policy would gate — but it is dropped.

**Risk**: A Cedar policy "max gauge LP withdrawal N per day" never fires for any Curve gauge `withdraw`.
This is the P1-3 class (user-controlled amount discarded), not a token mis-decode — the `tokenOut`
address is a correct literal. Severity P2 because the asset identity is right; only the magnitude lever
is missing.

**Remediation**: add `"amountOut.kind": {"literal":"exact"}`, `"amountOut.value": {"from":"$.args._value"}`
to the 3 `withdraw` bundles. No schema change — `ClaimUnstakeAction.amount_out` already exists.

---

### P2-3: crvUSD `liquidate_extended` discards `frac` and `callbacker`; `liquidate` overloads ignore `use_eth`

**Files**: `registry/manifests/curve/crvusd/{wsteth,sfrxeth,wbtc}/liquidateExtended@1.0.0.json`,
`.../liquidate-useEth@1.0.0.json` (6 bundles).

**Description**:
- `liquidate_extended(user, min_x, frac, use_eth, callbacker, callback_args)` — the bundle maps
  `borrower`, `debtToCover`←`min_x` (correct, post-P1-1) but drops `frac` (the fraction of the position to
  liquidate, 1e18-scaled) and `callbacker` (an arbitrary-call surface). A partial liquidation (`frac < 1e18`)
  is indistinguishable from a full one in the envelope.
- `liquidate(user, min_x, use_eth)` (`liquidate-useEth`) — `use_eth` is ignored. When `use_eth=true` the
  liquidator receives native ETH instead of WETH for the wstETH-derived collateral leg. The envelope's
  `collateralAsset` is a fixed ERC-20 literal regardless.

**Risk**: Policies keyed on liquidation size (`frac`) or on the callback target cannot be expressed.
Lower impact than P2-1 because liquidation is an action the *signer initiates against someone else's
position* — the signer is not the victim. Severity P2.

**Remediation**: forward-spec — `frac` would need a `LiquidateAction` field; `use_eth` a `bool_branch`
on `collateralAsset.kind`. Tracked as a Curve-dialect extension; not blocking.

---

### P2-4: `removeCollateral` overloads ignore `use_eth`; `add_collateral`/`remove_collateral` `_for` correctly routed

**Files**: `registry/manifests/curve/crvusd/{wsteth,sfrxeth,wbtc}/removeCollateral-useEth@1.0.0.json` (3 bundles).

**Description**: `remove_collateral(uint256 collateral, bool use_eth)` — the `removeCollateral-useEth`
bundle correctly maps `amount`←`collateral` and `recipient`←`$.tx.from`, action `withdraw`, but ignores
`use_eth`. As with P2-3, `use_eth=true` makes the withdrawn collateral leg native ETH (for ETH-derived
collateral) rather than the wrapped ERC-20 literal the envelope asserts.

Note (not a finding): the `add_collateral(uint256,address)` / `addCollateral-for` bundle and the
`repay-3arg` bundle correctly route the `_for` / recipient argument from calldata (`$.args._for`) — this
is the right behavior and matches the post-P0-2 expectation that user-controlled recipients flow from
calldata, not from `$.tx.from`. Verified clean.

**Risk**: A "withdraw only WETH"-style policy approves a call that returns native ETH. P2 — the asset is
economically near-equivalent (ETH ≈ WETH) and the amount lever is correct.

**Remediation**: forward-spec `bool_branch_emit` (shared with P2-3 / Phase-12 P2-4 cryptoswap `use_eth`).

---

## P3 — Low / Informational

### P3-1: `repayExtended` emits `amount.kind = "unknown"` — observability-only envelope

**File**: `registry/manifests/curve/crvusd/{wsteth,sfrxeth,wbtc}/repayExtended@1.0.0.json`.

`repay_extended(address callbacker, uint256[] callback_args)` carries no explicit debt amount in calldata
(the repaid amount is computed by the callback). The bundle honestly emits `amount.kind = "unknown"`.
This is the correct, non-deceptive choice — but a Cedar policy "max crvUSD repay N" cannot evaluate, and a
`callbacker` (arbitrary-call surface) is invisible. Informational: the bundle does not *mis*-state
anything, it under-states. No remediation required for PoC; flag the `callbacker` surface for the same
forward-spec callback-handling work as P2-1/P2-3.

### P3-2: All 105 Curve bundles claim `host:token_metadata` capability regardless of usage

Carry-forward of Phase-12 **P3-1**. The 65 new bundles continue the pattern — `gauge/*/withdraw`,
`gauge/*/claimRewards`, `crvusd/*/repay-1arg` etc. perform no on-chain metadata enrichment (all asset
addresses are literals) yet declare `requires.host_capabilities: ["host:token_metadata"]`. The capability
gate becomes a no-op signal. Minimize: bundles with fully-literal assets should declare `[]`.

### P3-3: 4x5 Router NG (`exchange-legacy`) per-hop `swap_type` semantics unconfirmed

`docs/PHASE13-CURVE-RESEARCH.md` §3 / §8.3 states that the `uint256[4][5]` `_swap_params` variant's exact
4-element per-hop layout could not be confirmed from a 1st-source (the deployed Fraxtal/Mantle/X-Layer/zkSync
routers diverge from the GitHub `fraxtal` tag source). **This does not affect the `exchange-legacy` bundle's
correctness** — the bundle decodes `_swap_params` as an opaque `uint256[4][5]` and never reads per-hop
fields; `curve_route_last_token` operates only on `_route` (the `address[11]` interleaved layout, which the
audit confirmed is identical for 4x5 routers via the `get_dx(address[11],uint256[4][5],uint256)` =
`0x7b5e2c7b` selector match). It only means a *future* per-hop-expansion bundle for 4x5 chains cannot be
written from current 1st-sources. Informational; no action this phase.

---

## Tier B verification detail (A.4)

### B1 — `CURVE_ROUTER_NG_ADDRESSES` +Kava(2222) +X-Layer(196)

Kava `0x0DCDED…d983` (shares the multi-chain CREATE2 address with OP/Gnosis/Polygon/Fantom/Avalanche),
X-Layer `0xBFab8ebc…1855`. Both match `docs/PHASE13-CURVE-RESEARCH.md` §1.1 (README @ `1014d369`). Table
is now 14 entries; the new `curve_router_ng_table_covers_14_chains` unit test asserts the count and the two
new entries. **PASS.** Note: the table is `(chain_id, address)` only and does not encode the 5x5/4x5 ABI
variant — correct, because (per the curve.rs comment) the ABI variant is the Tier A bundle's
`match.selector` responsibility, and the two bundles (`exchange` 0xc872a3c5 / `exchange-legacy` 0xaad348a2)
do split it correctly.

### B2 — `curve_route_last_token` rewrite (highest-risk change)

The pre-Phase-13 implementation returned the *last non-zero even-index slot* of `_route`. The Phase-13
rewrite mirrors `Router.vy::exchange`'s early-break loop. Verified line-by-line against the Vyper quoted in
`docs/PHASE13-CURVE-RESEARCH.md` §3:

```vyper
for i in range(5):
    swap: address = _route[i * 2 + 1]
    output_token = _route[(i + 1) * 2]
    ...
    if i == 4 or _route[i * 2 + 3] == empty(address):
        break
```

Rust (`builtin_fn.rs:389-403`):
```rust
let mut last_output_idx = 0usize;
for i in 0..5 {
    if is_zero_address(addr_str(arr, 2 * i + 1)?) { break; }
    last_output_idx = 2 * i + 2;
}
if last_output_idx == 0 || is_zero_address(addr_str(arr, last_output_idx)?) {
    return Err(FnError::EmptyRoute);
}
```

Equivalence proof:
- Vyper hop `i` sets `output_token = _route[2i+2]`; Rust sets `last_output_idx = 2i+2`. Identical.
- Vyper breaks *after* hop `i` when `_route[2i+3]` (next pool) is zero. Rust, at iteration `i+1`, tests
  `_route[2(i+1)+1] = _route[2i+3]` and breaks. Identical — the test is hoisted to the next iteration's
  top, semantically the same boundary.
- Vyper `i == 4` ceiling: hop 4's output is `_route[10]`. Rust at `i=4` (the last loop index) checks
  `_route[9]`; if non-zero, `last_output_idx = 10`. Identical.
- **Fail-closed additions** (not in Vyper, conservative-correct): if `_route[1]` (hop-0 pool) is zero, the
  Rust loop breaks immediately with `last_output_idx == 0` → `EmptyRoute`. On-chain, Vyper would still
  *enter* hop 0 with `swap = 0x0` and then revert inside the swap-type branch / the `approve(0x0,…)` —
  i.e. that route moves no value. Returning `EmptyRoute` (declarative miss → static fallback) instead of a
  bogus token is the safe choice. Likewise a zero resolved output slot → `EmptyRoute`.
- The two new regression tests (`gap_route_stops_at_first_zero_pool`, `zero_output_token_returns_error`)
  exercise exactly the P1-5 attack route and the degenerate case.

**PASS.** This was the Phase-12 **P1-5** finding and the fix is correct and complete.

### B3 — `enum_tagged` `curve_router_ng_swap_types` dispatcher wired

`lookup_dispatcher_table` previously returned `MapperError::Unsupported` for this id; Phase-13 wires it to
`CURVE_ROUTER_NG_SWAP_TYPES` (`enum_tagged.rs`). The new `curve_router_ng_dispatcher_is_wired` test drives
`swap_type = 8` (`WRAPPED_ASSET_CONVERT`) through the generic engine and asserts `kind`/`kind_name`. This
closes Phase-12 **P2-5**. No live bundle currently references the dispatcher (the Router NG bundles use
`single_emit` + `curve_route_last_token`), so the wiring is forward-spec scaffolding — but it is correct.
**PASS.**

---

## Carry-forward (deferred — NOT new P0/P1)

User-approved for a follow-up phase; require policy-engine core Cedar schema changes.

| ID | Carry-forward item | Phase-13 status |
|---|---|---|
| **P1-2** | crvUSD `create_loan` discards `collateral` deposit. **Phase-13 update**: the scope is now **9 bundles** — the 65-bundle expansion added `borrowMore` (3) and `createLoanExtended` (3) with the identical defect (see P2-1 above). The fix (two-envelope atomic intent) must cover all 9. | open — scope widened |
| **P1-3** | veCRV `increase_unlock_time` / `create_lock` / `increase_amount` discard `_unlock_time`; emit `stake` with `amountIn=0` for time-only ops. **Phase-13 note**: the gauge `withdraw` amount-loss (P2-2) is the same *class* but is a P2 (separate, smaller — gauge `withdraw` at least has the `tokenOut` literal right and `amountOut` is trivially addable without a schema change). | open |
| **P1-4** | GaugeController `vote_for_gauge_weights` puts `_gauge_addr` in `governance` (documented as the Governor contract). Needs a Curve-dialect `vote_target` field or an `Action::VoteOnTarget` variant. | open |
| **5×5 4-arg / 5-arg overload gap** | `docs/PHASE13-CURVE-RESEARCH.md` §2: Router NG `exchange` has three user-facing 5x5 overloads — 4-arg `0x371dc447`, 5-arg `0x5c9c18e2`, 6-arg `0xc872a3c5`. Phase-13 ships only the 6-arg bundle. The 4-arg/5-arg forms are unmapped → declarative miss → static fallback (no mis-decode, just no declarative coverage). The 4x5 variant likewise has only the 4-arg `0xaad348a2` shipped; its 5-arg `0x83cf75c8` / 6-arg `0xf0edc80e` are unmapped. **Not a P0/P1** — a miss fails safe to the static path. Listed as a coverage gap for a follow-up. | open — coverage gap |

---

## Trust-Failure Summary

| Finding | Surface | Cedar policy bypassed? | Discoverable by user? | Severity |
|---|---|---|---|---|
| P1-1 (cartesian callkey topology) | Router NG `exchange` + `exchange-legacy` on 52 spurious `(chain,addr)` pairs | Conditionally — if a spurious-pair contract is hit with `0xc872a3c5`/`0xaad348a2` | No — envelope claims a swap the target may not do | P1 |
| P1-2 (partial P1-6 fix) | frxETH `removeLiquidityImbalance-2` | Yes — `inputLp.asset.address` is null, address gate misses | Partially | P1 |
| P2-1 (borrow_more/createLoanExtended collateral lost) | 6 crvUSD bundles | Yes — no collateral lever | Partially (debt arm visible) | P2 |
| P2-2 (gauge withdraw amount lost) | 3 gauge withdraw bundles | Yes — no amount lever | No | P2 |
| P2-3 (liquidate_extended frac/callbacker; liquidate use_eth) | 6 crvUSD bundles | Conditionally | No | P2 |
| P2-4 (removeCollateral use_eth) | 3 crvUSD bundles | Conditionally (ETH vs WETH) | No | P2 |
| P3-1..P3-3 | Various | No (under-states, never mis-states) | N/A | P3 |
| A.1/A.2/A.3-core/A.4 | Router NG / liquidate / frxETH LP / Tier B | — | — | PASS |

---

## Sources

### 1st (file:line — this worktree)
- `registry/manifests/curve/router-ng/exchange@1.0.0.json`, `exchange-legacy@1.0.0.json`
- `registry/manifests/curve/crvusd/{wsteth,sfrxeth,wbtc}/*.json` (45 bundles)
- `registry/manifests/curve/{gauge,cryptoswap,stableswap,stableswap-ng}/**/*.json`
- `registry/scripts/build-index.ts:190,199-214` (cartesian product)
- `crates/policy-engine-wasm/src/declarative_exports.rs:99-113` (`register_bridge_entries`)
- `crates/adapters/mappers/src/declarative/builtin_fn.rs:336-404` (`curve_route_last_token`), `:437-481` (`select_from_literal_array`)
- `crates/adapters/abi-resolver/src/subdecode/protocols/curve.rs:56-105` (`CURVE_ROUTER_NG_SWAP_TYPES`), `:118-191` (`CURVE_ROUTER_NG_ADDRESSES`)
- `crates/adapters/mappers/src/declarative/enum_tagged.rs` (`lookup_dispatcher_table`)
- `crates/adapters/mappers/src/declarative/single_emit.rs:934-952` (`build_claim_unstake_envelope`)
- `AUDIT-PHASE12-CURVE.md`, `docs/PHASE13-CURVE-RESEARCH.md`

### 1st (external — Curve official + on-chain)
- `curvefi/curve-router-ng` @ `1014d3691bd9df935dc06fc5988484b0614d1fd5` — `README.md` (14-chain deploy list), `contracts/Router.vy::exchange` (early-break loop)
- `curvefi/curve-stablecoin` @ tag `v1` (`edbb5ef5`) — `contracts/Controller.vy` (`liquidate` `min_x` docstring, `repay` `int256 max_active_band`)
- On-chain probes (Foundry `cast` 1.5.1, `~/.foundry/bin/cast`): selector verification via `cast keccak` (Router NG `0xc872a3c5`/`0xaad348a2`/`0x371dc447`/`0x5c9c18e2`; `get_dx` `0x7b5e2c7b`; 13 crvUSD/gauge selectors); spurious-pair characterization via `cast code` / `cast call` on mainnet (publicnode/drpc), Optimism, Arbitrum, Polygon, Fraxtal, Mantle RPCs; frxETHCRV LP `symbol()`/`minter()` and crvUSD `symbol()` on Ethereum mainnet
- No Etherscan API key — contract identity at spurious pairs established by bytecode prefix / Vyper metadata trailer / revert-behavior fingerprint, consistent with `docs/PHASE13-CURVE-RESEARCH.md` §8 methodology

### Pattern references
- Phase-12 audit P0-2 — "calldata interpretation diverges from on-chain semantics → policy gate bypassed"
- Damn Vulnerable DeFi #11 (Backdoor) — trusted intermediary interpreting parameters differently from the implementation

---

## Remediation applied (post-audit, 2026-05-21)

| Finding | Action | Detail |
|---|---|---|
| **P1-1** cartesian callkey topology | ✅ FIXED (audit remediation option 2) | Router NG `exchange` 2 bundle → 9 per-chain bundle, mirroring the `uniswap/swap-router-02` per-chain pattern. Each bundle `to` = single address → `chain_ids × to` yields only real callkeys. Router NG callkeys: 66 (52 spurious) → **14 (0 spurious)**. Files: `registry/manifests/curve/router-ng/exchange@1.0.0.json` (rewritten — 6-chain `0x0DCDED…` shared-address group) + `exchange-{ethereum,bnb,base,arbitrum,fraxtal,mantle,xlayer,zksync}@1.0.0.json` (new); `exchange-legacy@1.0.0.json` removed. |
| **P1-2** partial P1-6 fix | ✅ FIXED | `stableswap/frxeth/removeLiquidityImbalance-2@1.0.0.json` — added `inputLp.asset.address` = `0xf43211935c781d5ca1a41d2041f397b8a7366c7a`. frxETH liquidity bundles now 4/4. |
| **P2-2** gauge `withdraw` amount lost | ✅ FIXED | `gauge/{3pool,steth,frxeth}/withdraw@1.0.0.json` — added `amountOut.kind=exact` / `amountOut.value`←`$.args._value`. |
| **P2-1 / P2-3 / P2-4** | ⏳ carry-forward | P2-1 folds into the deferred P1-2 (collateral two-envelope plumbing); P2-3/P2-4 need a `bool_branch_emit` strategy. User-approved for a follow-up phase. |
| **P3-1 / P3-2 / P3-3** | ⏳ carry-forward | Informational. |

Post-remediation: `cd registry && npm run build` → 706 callkey, 0 error. cargo `--workspace`
840/0/6 (unchanged — remediation is registry-JSON only). Phase gate (P0 = 0) holds.
