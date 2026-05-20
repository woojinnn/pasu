# Audit Report — Phase 12 Curve Tier A/B

**Audit date**: 2026-05-19
**Auditor**: solidity-auditor sub-agent (read-only)
**Scope**: Curve Protocol Phase 12.0-12.6 implementation (pre-P0 fix state)
**Worktree**: `phase7-uniswap-e2e`
**Plan**: `/Users/jhy/.claude-web3/plans/jazzy-tumbling-lighthouse.md`

---

## Executive Summary

Curve Tier A bundles (16 V1/V2/NG pool + 12 crvUSD + 4 veCRV + 6 Gauge + 1 GaugeController = **40 bundles**) and Tier B (`subdecode/protocols/curve.rs`, 7 new single_emit builders, `curve_route_last_token` BuiltinFn, enum_tagged_dispatch wiring) form the largest single-protocol expansion in the worktree.

**Pre-audit state** (Phase 12.0-12.6 완료 시점):
- cargo workspace 641/0/3 ignored
- vitest 205/0, tsc 0
- WASM 5.98 MiB / 6.00 MiB ceiling (19 KiB buffer)
- 371 callkey index files

**Findings**:
- **2 P0** — silent trust failure (Cedar bypass + envelope mislabel)
- **6 P1** — envelope semantic mismatch
- **5 P2** — incomplete coverage / overload gaps
- **3 P3** — best-practice deviations

**Post-audit P0 fix status**: ✅ P0-1 + P0-2 **모두 fix** 완료 (commit `10e8204`).
- 9 lowering modules + dispatch.rs 7 arm + exports.rs Verdict::Warn + schema.rs cedarschema wiring
- `select_from_literal_array` BuiltinFn + 10 bundle re-emission + 2 reverse-direction e2e test
- Post-fix: cargo workspace 660/0/3, WASM 5.916 MiB

---

## P0 — Critical (security trust failure)

### P0-1: All 7 new Curve actions silently bypass the Cedar policy engine

**Status**: ✅ FIXED in commit `10e8204`

**Files** (pre-fix state):
- `crates/policy-engine/src/lowering/dispatch.rs:81-99`
- `crates/policy-engine/src/lowering/mod.rs:1-9`
- `crates/policy-engine-wasm/src/exports.rs:722-727`
- `crates/policy-engine/src/policy/engine.rs:119` (empty input → Pass)

**Description**: `try_policy_request_from_envelope` in `dispatch.rs` matched only DEX (9 actions) and four `misc` actions (wrap/unwrap/transfer/permit). Every new Curve action variant — `Action::Borrow`, `Action::Repay`, `Action::Liquidate`, `Action::Stake`, `Action::ClaimUnstake`, `Action::ClaimRewards`, `Action::Vote` — hit the `_ => Ok(None)` fallback on line 98 and the WASM caller silently `continue`d past the envelope (exports.rs:722). Combined with `Verdict::aggregate(empty) → Verdict::Pass` (policy/verdict.rs:69-76 + engine.rs:119), every Curve crvUSD / veCRV / Gauge / GaugeController transaction defaulted to **Pass** regardless of installed Cedar policies. The `lowering/` directory contained zero modules for the `lending` / `staking` / `restaking` categories.

**Risk**: User installed a policy "deny crvUSD borrows > $X" or "no Curve gauge vote weight > 5000 bps" or "no stake longer than 1 week" — the policy was never evaluated. Silent fail-open: user has stronger expectations than the system actually delivers.

**Remediation (applied)**:
1. Added 9 new lowering modules (`lending/{borrow,repay,liquidate}.rs`, `staking/{stake,claim_unstake}.rs`, `misc/{vote,claim_rewards}.rs` + 2 mod.rs).
2. Extended `dispatch.rs:81-117` with 7 new match arms (Borrow / Repay / Liquidate / Stake / ClaimUnstake / Vote / ClaimRewards).
3. Wired 7 cedarschema files into `schema.rs` `base_schema_text()` (engine builder previously rejected `Action::"borrow"` etc. as unrecognized action).
4. Replaced silent `continue` at `exports.rs:697-740, 768-796` with synthetic `Verdict::Warn` carrying `MatchedPolicy { policy_id: "__engine::action_not_lowered::<kind>", severity: Warn, ... }` that aggregates with the policy verdict.
5. Added `crates/integration-tests/tests/p0_1_action_lowering.rs` (244 lines) — 7 forbid-policy tests + 1 negative test (supply still returns None). **8/8 pass**.

---

### P0-2: Stableswap V1/V2/NG pool exchange bundles silently mislabel (input, output) tokens for any (i, j) ≠ hardcoded pair

**Status**: ✅ FIXED in commit `10e8204`

**Files** (pre-fix state):
- `registry/manifests/curve/stableswap/3pool/exchange-v1@1.0.0.json:28-33` (DAI/USDC literal)
- `registry/manifests/curve/stableswap/steth/exchange-v1@1.0.0.json:29-33` (ETH/stETH literal)
- `registry/manifests/curve/stableswap/frxeth/exchange-v1@1.0.0.json:28-33`
- `registry/manifests/curve/cryptoswap/tricrypto2/exchange-v2@1.0.0.json:30-35` (USDT/WBTC literal; ignores `use_eth`)
- `registry/manifests/curve/cryptoswap/tricryptousdc/exchange-v2@1.0.0.json`
- `registry/manifests/curve/stableswap-ng/crvusd-usdc/exchange-ng@1.0.0.json:28-37`
- `registry/manifests/curve/stableswap-ng/crvusd-usdt/exchange-ng@1.0.0.json`
- `registry/manifests/curve/stableswap/{3pool,steth,frxeth}/removeLiquidityOneCoin@1.0.0.json:33-37`

**Description**: Every V1/V2/NG pool `exchange` bundle hardcoded `inputToken.asset.address` and `outputToken.asset.address` as fixed literals, while the actual on-chain function takes `(i, j)` indices that the user fully controls. Concretely:

- 3pool exchange bundle claimed **input = DAI, output = USDC** regardless of what `(i, j)` the calldata carried. A user swapping USDT→DAI (i=2, j=0), USDC→USDT (i=1, j=2), etc., got an envelope falsely asserting **DAI→USDC**.
- steth exchange claimed native ETH input + stETH output. For `j=0, i=1` (stETH→ETH), the bundle still claimed `inputToken.kind="native"` + `outputToken=stETH`, both inverted.
- tricrypto2 V2 exchange claimed USDT→WBTC, ignoring `use_eth: bool`.
- stableswap-NG (crvusd-usdc) claimed crvUSD→USDC; the (1,0) direction silently mislabeled.
- `remove_liquidity_one_coin` hardcoded outputs to `coins[0]` regardless of user-supplied `i` argument.

**Risk**: A user lured into signing a Curve calldata with attacker-chosen `(i, j)` (e.g. a malicious dApp that sets `j=2` to swap into USDT) saw their policy approve the transaction based on the bundle's hardcoded token pair. Silent — no warning, no degradation flag. For the steth pool the cost was concrete: user policy "only allow swapping ETH (limit 1 ETH)" saw the bundle assert ETH input + 1 ETH amount and approved; calldata actually swapped 1 stETH (an ERC20 the user has 100 of) → ETH, draining the wallet.

This is a Solodit/Code4rena pattern: "calldata interpretation diverges from on-chain semantics → policy gate bypassed". cf. Damn Vulnerable DeFi #11 (Backdoor) — same root cause: trusted intermediary interpreted parameters differently from the implementation.

**Remediation (applied)**:
1. New `BuiltinFn::SelectFromLiteralArray` variant in `crates/adapters/mappers/src/declarative/types.rs:296-305`.
2. `select_from_literal_array` impl + `coerce_to_i64` helper in `builtin_fn.rs:257-317` (9 unit tests).
3. eval.rs:165-180 wire-up in `evaluate_transform`.
4. bundle-schema.ts:108-131 — added `"select_from_literal_array"` to `BuiltinFn` union + `ALL_BUILTIN_FNS` set.
5. 10 Curve bundles re-emitted (3 V1 exchange + 2 V2 exchange + 2 NG exchange + 3 removeLiquidityOneCoin) — `inputToken.asset.address` + `outputToken.asset.address` now use `select_from_literal_array(coins[], $.args.i_or_j)`.
6. 2 new reverse-direction e2e tests:
   - `declarative_curve_3pool_exchange_v1_usdt_to_dai` (i=2, j=0)
   - `declarative_curve_steth_exchange_v1_steth_to_eth` (i=1, j=0)

---

## P1 — High (incorrect envelope semantics on declarative path)

> **Status**: P1 findings = **forward-spec scope** (plan §11 expand 의 후보). Not blocking PoC closure.

### P1-1: `liquidate` bundle maps `min_x` (minimum debt-token received) to `seizedCollateralAmount`

**Files**:
- `registry/manifests/curve/crvusd/wsteth/liquidate@1.0.0.json:33-34`
- `registry/manifests/curve/crvusd/wsteth/selfLiquidate@1.0.0.json:32-33`
- `registry/manifests/curve/crvusd/{sfrxeth,wbtc}/{liquidate,selfLiquidate}@1.0.0.json` (4 bundles, same pattern)
- `crates/adapters/mappers/src/declarative/mapper.rs:1743-1751` (test enshrines the wrong semantic)

**Description**: Curve crvUSD Controller `liquidate(address _user, uint256 _min_x, ...)` has, per the Vyper source (`curvefi/curve-stablecoin/curve_stablecoin/controller.vy:1223` docstring):

> `_min_x` — Minimal amount of borrowed asset to receive (to avoid liquidators being sandwiched)

That is, `min_x` is the **minimum debt-asset (crvUSD) the liquidator receives**. The bundle maps:

```json
"seizedCollateralAmount.kind":  { "literal": "min" },
"seizedCollateralAmount.value": { "from": "$.args.min_x" }
```

So the envelope reports `seized_collateral_amount = min_x`, but `seized_collateral_amount` is the schema's "collateral seized" field (in wstETH/sfrxETH/WBTC). A Cedar policy "max liquidation reward 10 wstETH" would compare the wrong value. The conceptually correct mapping is to `debtToCover` with `kind = min`.

**Risk**: Cedar policies authored by liquidator bots / risk managers compare amount thresholds against the wrong asset and the wrong dimensions. wstETH ≈ 18 dp, crvUSD ≈ 18 dp, but the values are not interchangeable — 1 wstETH is worth ~$3000 of crvUSD.

**Recommendation**: Re-map all 6 liquidate / selfLiquidate bundles to:
```json
"debtToCover.kind":  { "literal": "min" },
"debtToCover.value": { "from": "$.args.min_x" }
```
And update the `declarative_curve_crvusd_wsteth_liquidate` test fixture to assert `action.debt_to_cover` instead of `action.seized_collateral_amount`.

---

### P1-2: crvUSD `create_loan` envelope discards the `collateral` input entirely

**Files**:
- `registry/manifests/curve/crvusd/wsteth/createLoan@1.0.0.json:22-36`
- `registry/manifests/curve/crvusd/{sfrxeth,wbtc}/createLoan@1.0.0.json`

**Description**: `create_loan(uint256 collateral, uint256 debt, uint256 N)` deposits `collateral` wstETH/sfrxETH/WBTC **and** borrows `debt` crvUSD in a single atomic call. The bundle emits a single `BorrowAction` carrying only `amount = debt`. The `collateral` argument (the collateral being deposited from the user's wallet into the Controller) is **never represented in the envelope**. The `N` parameter (number of LLAMMA bands) is also discarded.

**Risk**:
- Cedar policy "max wstETH collateral 100" / "min N=10 bands" cannot be enforced — the relevant fields don't exist in the envelope.
- Borrowing 10000 crvUSD against 0.01 wstETH (suicidal LTV) is indistinguishable from 10000 crvUSD against 1000 wstETH in the envelope.

**Recommendation**: Emit a **two-envelope atomic intent** (Aerodrome-style multi-envelope):
1. `Action::Borrow { asset: crvUSD, amount: debt }`
2. `Action::Supply { asset: wstETH, amount: collateral }`

…and lift `N` into envelope-level metadata.

---

### P1-3: veCRV `increase_unlock_time` emits `stake` envelope with `amountIn=0`

**Files**:
- `registry/manifests/curve/vecrv/increaseUnlockTime@1.0.0.json:25-31`
- `registry/manifests/curve/vecrv/createLock@1.0.0.json:22-32`
- `registry/manifests/curve/vecrv/increaseAmount@1.0.0.json`

**Description**: `increase_unlock_time(uint256 _unlock_time)` extends the duration of an existing veCRV lock with no token movement. The bundle emits `amountIn = 0` which the Cedar `stake` action sees as a 0-CRV stake. The actual `_unlock_time` argument (Unix timestamp) is **not propagated** to the envelope. Similarly `create_lock(_value, _unlock_time)` and `increase_amount(_value)` discard `_unlock_time` entirely — only `_value` flows to `amountIn.value`.

**Risk**:
- User policy "do not lock CRV longer than 1 year" cannot be enforced; duration not in envelope.
- "Always 4-year lock for max voting power" policy cannot be enforced.
- `amountIn = 0` for `increase_unlock_time` means a policy "no stake > 1000 CRV" approves trivially.

**Recommendation**:
1. Add `validity.unlock_time` (or new `Validity::lock_expiry`) field populated from `$.args._unlock_time`.
2. Alternative: introduce `ExtendStake` variant (plan §11 forward-spec).

---

### P1-4: GaugeController `vote_for_gauge_weights` maps `_gauge_addr` to `governance`

**Files**:
- `registry/manifests/curve/gauge-controller/voteForGaugeWeights@1.0.0.json:26-30`
- `crates/policy-engine/src/action/misc/vote.rs:13` (`governance: Address` documented as "Governor contract")
- `schema/policy-schema/actions/misc/vote.cedarschema:1-19`

**Description**: VoteAction's `governance` field is documented as `Governor contract`. The Curve bundle puts `$.args._gauge_addr` (the gauge being weighted) into `governance`, while the actual governor (GaugeController, `0x2F50...46bB`) goes only into the **string-typed `governanceLabel`**.

The mapper test `mapper.rs:1898-1903` openly asserts this misuse:
```rust
assert_eq!(action.governance, curve_3pool_gauge());
assert_eq!(action.governance_label.as_deref(), Some("Curve GaugeController"));
```

**Risk**:
- User Cedar policy "allow votes only on GaugeController" sees `context.governance == <gauge>` and denies all votes — false negative DoS.
- Alternative misuse: user writes "allow votes only on 3pool gauge". `_user_weight` field uncapped at envelope level.

**Recommendation**: Either keep `governance = GaugeController` and add Curve-dialect `vote_target: AddressRef` for `_gauge_addr`. Or split: add `Action::VoteOnTarget { governance, target, weight }`.

---

### P1-5: `curve_route_last_token` ignores early-termination semantics in Router NG

**Files**:
- `crates/adapters/mappers/src/declarative/builtin_fn.rs:218-248`
- `registry/manifests/curve/router-ng/exchange@1.0.0.json:40-44`

**Description**: `curve_route_last_token` scans even indices 0, 2, 4, 6, 8, 10 of the 11-slot `_route` array and returns the last non-zero. Per `curve-router-ng/contracts/Router.vy::exchange`, the on-chain code **iterates with early break**:
```vyper
if i == 4 or _route[i * 2 + 3] == empty(address):
    break
```
The swap **stops** at the first zero pool address (odd index). Tokens after that zero are never touched. If an attacker crafts a route `[A, P1, B, 0x0_pool, MID, P3, D, ..., E]`, on-chain execution stops at `B` (A→B only), but `curve_route_last_token` returns `E`.

**Risk**:
- User Cedar policy "swap output must be E" approves; on-chain, user ends at `B`.
- Attack vector requires malicious dApp / router-frontend to construct calldata where the first zero pool sits after the intended output token.

**Recommendation**:
1. Update `curve_route_last_token` to mirror Router.vy's iteration logic — track the **last hop with non-zero pool**, return that hop's destination token.
2. Add unit test exercising the gap case.
3. Optional: fail-closed `FnError::CorruptedRoute` if any even-index zero appears before inferred output.

---

### P1-6: frxETH pool bundles omit `inputLp.asset.address` / `outputLp.asset.address` literals

**Files**:
- `registry/manifests/curve/stableswap/frxeth/addLiquidity-2@1.0.0.json:35-37`
- `registry/manifests/curve/stableswap/frxeth/removeLiquidity-2@1.0.0.json:28-30`
- `registry/manifests/curve/stableswap/frxeth/removeLiquidityOneCoin@1.0.0.json:29-31`
- `crates/adapters/mappers/src/declarative/single_emit.rs:1619-1629` (read_asset_inline accepts address=None silently)

**Description**: Three frxETH-ETH pool bundles declare `inputLp.asset.kind = "erc20"` but never set `inputLp.asset.address` (no `literal`, no `from`). `read_asset_inline` allows `address` to be `None` without error.

**Risk**: Cedar policies "only burn LP tokens of pool X" cannot match — address field is null. The frxETH pool (`0xa1F8...E577`) uses the pool contract itself as the LP token.

**Recommendation**:
1. Fix the 3 frxETH bundles: set `inputLp.asset.address` / `outputLp.asset.address` = `"0xa1f8a6807c402e4a15ef4eba36528a3fed24e577"`.
2. Harden `read_asset_inline` to reject `AssetKind::Erc20` with `address: None` — make it `MissingField(field.address)`.

---

## P2 — Medium (incomplete coverage / overload gaps)

### P2-1: `Action::Repay` is not validated for repay overload alignment with deployed crvUSD Controllers

**File**: `registry/manifests/curve/crvusd/wsteth/repay@1.0.0.json:5-23` (selector `0x79a5f4dc`)

**Description**: Bundle matches selector `0x79a5f4dc` = `repay(uint256,address,uint256,bool)`. Vyper Controller `controller.vy:1069` defines:
```vyper
def repay(_wallet_d_debt: uint256, _for: address = msg.sender,
          _max_active_band: int256 = max_value(int256),
          _callbacker: address = empty(address),
          _calldata: Bytes[CALLDATA_MAX_SIZE] = b"",
          _shrink: bool = False):
```
`_max_active_band: int256` in Vyper vs `"uint256"` in bundle ABI fragment — possible mismatch.

**Risk**: If deployed Controller's `repay` doesn't actually expose selector `0x79a5f4dc`, bundle never matches — silent miss. Conversely, if ABI fragment decodes `_max_active_band` as `uint256`, `int256::MIN`-valued args decode as huge positives.

**Recommendation**: Run `cast call <controller> 'repay(uint256,address,int256,address,bytes,bool)' ...` against mainnet or `cast 4byte-decode` on known historical tx hash to confirm. Same audit needed for `create_loan` (3-arg vs 6-arg per Vyper defaults).

---

### P2-2: Curve Controller overloads beyond hardcoded selector are not modeled

**Files**:
- `registry/manifests/curve/crvusd/wsteth/createLoan@1.0.0.json:6-9` (only 3-arg `create_loan` selector `0x23cfed03`)
- `registry/manifests/curve/crvusd/wsteth/liquidate@1.0.0.json` (only 2-arg `liquidate`)

**Description**: Curve's Controller exposes multiple overloads per Vyper default-arg generation:
- `create_loan(uint256,uint256,uint256)` = `0x23cfed03` — mapped
- `create_loan(uint256,uint256,uint256,address)` = `0xfadc9bfb` — **not mapped**
- `create_loan(uint256,uint256,uint256,address,address,bytes)` = `0x3003e089` — **not mapped**
- `liquidate(address,uint256)` = `0xbcbaf487` — mapped
- `liquidate(address,uint256,uint256)` = `0x0710285c` — **not mapped**
- `liquidate(address,uint256,uint256,address,bytes)` = `0x2a850ca1` — **not mapped**
- `borrow_more(...)` (5 selectors) — **not mapped at all**

**Risk**:
- Users calling `create_loan(..., _for=otherUser)` get no envelope match.
- Callbacker variants (leverage-via-callback path) invisible — token-drain pattern through attacker-controlled `_callbacker` passes through with no envelope.
- `borrow_more` (ongoing-position debt-increase path) unmapped.

**Recommendation**: For PoC closure: document in plan §11. For Phase 13+: extend each Controller's bundle set OR introduce selector-prefix-match fallback (deny-by-default).

---

### P2-3: Gauge `withdraw(uint256)` is not modeled

**Files**: `registry/manifests/curve/gauge/{3pool,steth,frxeth}/` (deposit + claimRewards only)

**Description**: Gauge cluster ships `deposit(uint256)` (selector `0xb6b55f25`) and `claim_rewards()` (selector `0xe6f1daf2`) per gauge. The natural counterpart `withdraw(uint256)` (selector `0x2e1a7d4d`) and `claim_rewards(address)` overload (selector `0x84e9bd7e`) are **not registered**.

**Risk**: User installs "max stake withdrawal 10 LP per day" — never fires for any Gauge.withdraw call.

**Recommendation**: Add 3 × `withdraw@1.0.0.json` bundles using `ClaimUnstake` builder. Add `claim_rewards(address)` overload.

---

### P2-4: Cryptoswap V2 `exchange` ignores `use_eth`

**Files**:
- `registry/manifests/curve/cryptoswap/tricrypto2/exchange-v2@1.0.0.json:32-35`
- `registry/manifests/curve/cryptoswap/tricryptousdc/exchange-v2@1.0.0.json`

**Description**: Cryptoswap V2 `exchange(uint256 i, uint256 j, uint256 dx, uint256 min_dy, bool use_eth)` has `use_eth: bool`. When `use_eth=true`, the pool deposits/withdraws native ETH instead of WETH. The bundle ignores `use_eth` entirely.

**Risk**: User asks for native ETH output via `use_eth=true`. Envelope says WBTC. Cedar policy "approve only WBTC withdrawals" approves — actual receipt is ETH.

**Recommendation**: Forward-spec `bool_branch_emit` strategy (plan §11). PoC stop-gap: `outputToken.asset.kind` from a `select` on `use_eth`.

---

### P2-5: `enum_tagged_dispatch` dispatcher_id `curve_router_ng_swap_types` remains an `Unsupported` stub

**File**: `crates/adapters/mappers/src/declarative/enum_tagged.rs:96-100`

**Description**: `lookup_dispatcher_table` hardcodes `Err(MapperError::Unsupported(...))` for the Curve Router NG dispatcher. The static EnumTable `CURVE_ROUTER_NG_SWAP_TYPES` is defined at `subdecode/protocols/curve.rs:56-105` and tested in isolation, but the wiring from the dispatcher to that table is **missing**. The Router NG bundle sidesteps the dispatcher by using single_emit + `curve_route_last_token`.

**Risk**: Forward-spec dependency. Phase 13+ when per-hop dispatch is enabled, any bundle referencing `dispatcher_id: "curve_router_ng_swap_types"` errors out.

**Recommendation**: Replace lines 96-100 with `"curve_router_ng_swap_types" => Ok(&CURVE_ROUTER_NG_SWAP_TYPES)`, add `use` import. Add integration unit test driving router_ng swap_type=8 payload.

---

## P3 — Low / Informational

### P3-1: All Curve bundles claim `host:token_metadata` capability regardless of usage

**Files**: All 40 bundles in `registry/manifests/curve/**/*.json` declare `requires.host_capabilities: ["host:token_metadata"]`.

**Description**: `vecrv/withdraw@1.0.0`, `gauge/3pool/claimRewards@1.0.0`, `gauge-controller/voteForGaugeWeights@1.0.0`, `vecrv/increaseUnlockTime@1.0.0`, and others perform no on-chain token metadata enrichment — all asset addresses hardcoded literals.

**Risk**: The host capability gate becomes a no-op signal. Cedar policy "deny bundles that require `host:token_metadata`" trivially denies all 40.

**Recommendation**: Minimize the capability surface — bundles with hardcoded literals should declare `host_capabilities: []`.

---

### P3-2: `_user_weight` upper bound (10000 bps) is not enforced in the envelope

**File**: `registry/manifests/curve/gauge-controller/voteForGaugeWeights@1.0.0.json:30`

**Description**: Curve's GaugeController asserts `_user_weight <= 10000`. The bundle maps `_user_weight` to `votingPower: Option<DecimalString>` without any range validation. Accidentally passing `weight=99999` produces envelope with `votingPower = "99999"`; on-chain call reverts, but policy engine sees value out of bounds with no normalization.

**Risk**: Cedar policy "vote weight ≤ 5000 bps" needs to interpret `votingPower` as bps. Typo `weight=50000` slips past poorly-written policy.

**Recommendation**: Add Cedar-context `votingPowerBps: Long` projection in future `lowering/misc/vote.rs` and enforce `0 ≤ votingPower ≤ 10000`. Default Cedar policy template should include `forbid when context.votingPower > 10000`.

---

### P3-3: Static `CRVUSD_CONTROLLERS` table covers only 3 mainnet markets

**Files**:
- `crates/adapters/abi-resolver/src/subdecode/protocols/curve.rs:202-227`
- `registry/manifests/curve/crvusd/` (3 markets × 4 funcs = 12 bundles)

**Description**: PoC scope is 3 crvUSD markets (wstETH / sfrxETH / WBTC). Curve has additional production markets (per `crvusd.curve.finance/v2/markets`). Calls to non-PoC markets bypass declarative path.

**Risk**: User borrowing crvUSD against a non-PoC collateral type bypasses declarative + Cedar entirely (compounds P0-1).

**Recommendation**: Document explicit per-market scope per Phase. Forward-spec: introduce `host:crvusd_factory` capability.

---

## WASM Size Verification

| Artifact | Path | Size (bytes) | Size (MiB) |
|---|---|---|---|
| Raw release output | `target/wasm32-unknown-unknown/release/policy_engine_wasm.wasm` | 8,658,651 | 8.258 |
| Post-`wasm-pack` (browser load size) | `crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm` | 6,272,005 | **5.981** |
| Deployed to extension (pre-P0 fix) | `browser-extension/dist/chrome/wasm/policy_engine_wasm_bg.wasm` | 6,250,585 | 5.961 |
| **Post-P0-1 + P0-2 fix** (wasm-opt -Oz) | `crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm` | 6,203,194 | **5.916** |

**Ceiling**: 6.000 MiB (6,291,456 bytes).

**Buffer remaining (post-fix)**: 6,291,456 − 6,203,194 = **88,262 bytes ≈ 85.0 KiB**.

**Conclusion**: Phase 13+ extensions must operate inside this narrow margin. Phase 12's additions consumed the bulk of pre-existing slack. Risk: **TIGHT** — a single thoughtlessly-introduced large dependency (e.g. ethers / regex / serde_with) can blow the budget.

**Recommendation**:
1. Block-budget: CI check `target/wasm32-unknown-unknown/release/policy_engine_wasm.wasm` post-pack size must be ≤ 6,000,000 bytes (red-line at 5,900,000 for "buffer healthy"). Fail PR on regression.
2. Before Phase 13: run `twiggy top -n 30` / `twiggy paths` against the wasm to identify candidates for `#[cfg(not(target_arch="wasm32"))]` gating.
3. Defer until needed: P3-2 / P3-3 fixes should not be undertaken if they add code without removing equivalent.

---

## Missing Test Coverage (plan §7 Phase 12.7 Step 1)

The plan §7 enumerated 6 edge-case test sub-agents. **None of these fixture files are present** in `crates/adapters/mappers/tests/fixtures/`:

- `T-TEST-CURVE-STABLESWAP`: V1 (i, j) ≠ (0, 1) — **absent** (P0-2 regression test, partially covered by post-fix `declarative_curve_3pool_exchange_v1_usdt_to_dai` + `declarative_curve_steth_exchange_v1_steth_to_eth`)
- `T-TEST-CURVE-STABLESWAP`: V2 `use_eth=true` — **absent** (P2-4 regression test)
- `T-TEST-CURVE-ROUTER-NG`: zero-padded gap route — **absent** (P1-5 regression test)
- `T-TEST-CURVE-LIQUIDITY`: zero-deposit fault — **absent**
- `T-TEST-CURVE-CRVUSD`: N=0 / N=50 / zero-debt / max-collateral — **absent**
- `T-TEST-CURVE-VECRV`: `_unlock_time < now` / max-cap (4 years) — **absent** (P1-3 regression test)
- `T-TEST-CURVE-GAUGE-VOTE`: `_user_weight > 10000` — **absent** (P3-2 regression test)

---

## Trust-Failure Summary

| Finding | Surface | Cedar policy bypassed? | Discoverable by user? | Status |
|---|---|---|---|---|
| P0-1 (lowering missing) | All Curve actions except DEX | Yes — every borrow/repay/liquidate/stake/vote/claim | No — silent skip, no log | ✅ FIXED |
| P0-2 (i,j hardcoded) | All V1/V2/NG pool exchange + remove_liquidity_one_coin | Conditionally yes (when actual ≠ (0,1)) | No — envelope claims wrong tokens | ✅ FIXED |
| P1-1 (min_x → seizedCollateral) | All 6 liquidate / selfLiquidate bundles | Yes (wrong amount field) | No | ⏳ forward-spec |
| P1-2 (create_loan collateral lost) | All 3 createLoan bundles | Yes (no collateral lever) | Partially (debt arm visible) | ⏳ forward-spec |
| P1-3 (unlock_time lost) | createLock / increase_unlock_time / increase_amount | Yes (no duration lever) | No | ⏳ forward-spec |
| P1-4 (governance = gauge) | voteForGaugeWeights | Conditionally — depends on policy author intent | Partially | ⏳ forward-spec |
| P1-5 (route gap output) | router-ng/exchange | Conditionally (adversarial route) | No | ⏳ forward-spec |
| P1-6 (frxETH LP address null) | 3 frxETH bundles | Yes (address gate misses) | Partially | ⏳ forward-spec |
| P2-1 to P2-5 | Various | Conditionally | Sometimes | ⏳ forward-spec |
| P3-1 to P3-3 | Various | No (best practice / informational) | N/A | ⏳ forward-spec |

---

## Sources

### 1차 (file_path:line_number, 본 worktree)
- `crates/policy-engine/src/lowering/dispatch.rs:81-99`
- `crates/policy-engine/src/lowering/mod.rs:1-9`
- `crates/policy-engine-wasm/src/exports.rs:722-727`
- `crates/policy-engine/src/policy/engine.rs:119`
- `crates/adapters/mappers/src/declarative/builtin_fn.rs:218-248`
- `crates/adapters/mappers/src/declarative/enum_tagged.rs:96-100`
- `crates/adapters/abi-resolver/src/subdecode/protocols/curve.rs:56-105, 202-227`
- `crates/policy-engine/src/action/misc/vote.rs:13`
- `crates/adapters/mappers/src/declarative/single_emit.rs:1619-1629`
- All Curve bundles under `registry/manifests/curve/**`

### 1차 (외부, Curve official)
- `curvefi/curve-router-ng @ master / contracts/Router.vy::exchange` (early-break iteration)
- `curvefi/curve-stablecoin/curve_stablecoin/controller.vy:1069, 1223` (liquidate docstring, repay signature)
- `curvefi/curve-dao-contracts/master/contracts/GaugeController.vy::vote_for_gauge_weights` (10000 bps assertion)

### Pattern references
- Damn Vulnerable DeFi #11 (Backdoor) — trusted intermediary interpreting parameters differently from implementation
- Solodit/Code4rena category: "calldata interpretation diverges from on-chain semantics → policy gate bypassed"

---

## Remediation Files for P0-1 + P0-2 (applied)

**P0-1 files modified** (commit `10e8204`):
- `crates/policy-engine/src/lowering/lending/{mod,borrow,repay,liquidate}.rs` (new, 228 lines total)
- `crates/policy-engine/src/lowering/staking/{mod,stake,claim_unstake}.rs` (new, 68 lines)
- `crates/policy-engine/src/lowering/misc/{vote,claim_rewards}.rs` (new, 138 lines)
- `crates/policy-engine/src/lowering/mod.rs` (added `pub mod lending; pub mod staking;`)
- `crates/policy-engine/src/lowering/dispatch.rs:81-117` (7 new match arms)
- `crates/policy-engine/src/schema.rs:41-58, 165-174` (7 cedarschema files wired into base_schema_text())
- `crates/policy-engine-wasm/src/exports.rs:697-740, 768-796` (Verdict::Warn synthesis)
- `crates/integration-tests/tests/p0_1_action_lowering.rs` (new, 244 lines, 8 tests pass)

**P0-2 files modified**:
- `crates/adapters/mappers/src/declarative/types.rs:296-305` (BuiltinFn::SelectFromLiteralArray variant)
- `crates/adapters/mappers/src/declarative/builtin_fn.rs:257-317` (impl + 9 unit tests)
- `crates/adapters/mappers/src/declarative/eval.rs:165-180` (wire-up)
- `browser-extension/backend/service-worker/marketplace/bundle-schema.ts:108-131` (whitelist)
- 10 Curve bundles re-emitted: 3 V1 exchange + 2 V2 exchange + 2 NG exchange + 3 removeLiquidityOneCoin
- `crates/adapters/mappers/src/declarative/mapper.rs:1539-1614` (2 new reverse-direction e2e tests)

---

## Audit Trail

- **2026-05-19**: Audit run by `solidity-auditor` sub-agent (read-only) post Phase 12.0-12.6 implementation
- **2026-05-19**: P0-1 + P0-2 fix applied (sub-agent dispatch, 220 tool uses, 1705s duration)
- **2026-05-19**: Final commit `10e8204` (worktree-phase7-uniswap-e2e branch, local only — not pushed)
- **2026-05-19**: This audit report saved to `AUDIT-PHASE12-CURVE.md` per user request
