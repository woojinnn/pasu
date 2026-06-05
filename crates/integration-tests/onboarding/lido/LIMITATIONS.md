# Lido onboarding — known limitations & remediation plan

> Honest gap list for the `feat/lido-onboarding` run (worktree `scopeball-lido`, off
> `feat/registry-v2`; base after P0–P4 = `df15cb65`). P0–P4 are `done` and
> `check-onboarding-evidence --phase all` = 70 rows / 0 blocked, but "done" means the
> *covered* surface is gate-clean — it does **not** mean zero limitations. This file
> is the next-work spec. Each item: what / why it matters / materiality / approach /
> files / gate. Close items by updating this file + `evidence.md` and committing.
>
> Categories: **Addressable** (fix), **Larger** (real work but separate scope —
> needs a user decision before starting), **Structural** (ScopeBall design choices —
> document as accepted, do NOT fake a fix).

---

## STATUS (2026-06-03) — all Addressable items resolved

**L1 ✅ L2 ✅ L3 ✅ L4 ✅ L5 ✅** — committed on `feat/lido-onboarding`
(`fcf1a473` L1 · `b8a87038` L2 · `8280ad57` L3 · `74c08354` L4 · `9c0655af` L5),
each gate-checked + evidence-updated. **Larger (L6 enrichment-RPC, L7 multichain)
remain DEFERRED — separate scope, awaiting a user decision (not started).
Structural items remain accepted-by-design (not faked).** Not pushed/merged.

---

## Addressable — fix these (priority order)

### L1 — bare-ETH selectorless stake  ✅ RESOLVED (was HIGH)
> **Resolution (this run, manifest+gate+loader — NO core-decoder change):** the WASM
> route already supported the selector-less path (`declarative_exports.rs`
> `NATIVE_TRANSFER_SELECTOR = "0x00000000"`: empty calldata → sentinel lookup → empty
> args, body reads `$tx.value`); the gap was purely (a) no Lido sentinel manifest, and
> (b) the extension loader pre-gating empty calldata before the WASM route. Landed:
> - **2 manifests** `manifests/lido/{steth,wsteth}/stake-eth@1.0.0.json` keyed on the
>   `0x00000000` sentinel → emit `liquid_staking.stake { venue: lido, amount: $tx.value }`.
> - **2 coverage entries** (`steth/wsteth.coverage.json` `0x00000000` = cover) + a
>   **surface-gate enhancement** (`check-surface-completeness.ts`: `hasNativeEntrypoint` +
>   I1' exemption) so the sentinel is grounded in the snapshot's payable `fallback()` /
>   `receive()` entry instead of a function-selector — a structural gate gap, not a hack.
> - **2 real-tx corpus entries** (`0x651d8e2d…` ETH→stETH, `0xf5ef5c38…` ETH→wstETH),
>   `expect_body` pinning `action=stake` + `amount=$tx.value`. `corpus --filter lido
>   --require-expect-body` = **11/11**.
> - **loader wire-up** (`declarative-route.ts` `nativeTransferSelector`): a value-bearing
>   empty-calldata tx synthesizes the `0x00000000` sentinel (gated on value>0); a
>   non-sentinel address still misses → same warn as before (one extra registry lookup).
>   3 new vitest cases; `tsc --noEmit` clean; full extension suite green.
>
> **Materiality of the fix:** the bare-ETH stake now decodes to `liquid_staking.stake`
> (real verdict) instead of a fail-closed `warn`. The wstETH variant's stake-then-wrap is
> modeled as `stake` (primary intent); the wstETH-vs-stETH output token is a wrap
> refinement not represented in `StakeAction` (acceptable — noted in the manifest).
>
> ---
>
> - **What:** sending ETH with **empty calldata** to stETH (`Lido` fallback, `NON_EMPTY_DATA`-guarded) or to wstETH (`receive()`, stakes+wraps in one step) mints the staking token. These txs have **no 4-byte selector**, so the `(chain,to,selector)` router misses them and the orchestrator fail-closes to a generic `warn`. The user sees "unknown action" instead of "you stake X ETH".
- **Why it matters:** it is a genuine wallet-facing stake path that ScopeBall cannot name. Conservative (warn) but zero decode value.
- **Materiality:** measured ~0.06% of direct txs (17 / 30 000 in the P2 sweep) — small, but it is the one concrete *decode* gap, not a scope choice.
- **Approach:** FIRST investigate whether empty-calldata / native-sentinel routing is even expressible. The harness `coverage` output lists `native-transfer sentinel 0x00000000` as a deferred category, and `harness/mod.rs::replay` special-cases `selector == "0x00000000"` — so a sentinel concept exists. Check `crates/policy-engine-wasm/src/declarative_exports.rs` (route resolution) + the registry callkey scheme (`registryV2/scripts/build-index.ts`) for whether a manifest can key on selector `0x00000000` + a value-bearing tx.
  - If expressible → add 2 manifests (`manifests/lido/steth/stake-eth`, `manifests/lido/wsteth/stake-eth`) keyed on the empty-calldata sentinel, emit `liquid_staking.stake` with `amount=$tx.value` (wstETH variant may need a `wrap`-after-stake note), plus a real-tx corpus entry each.
  - If NOT expressible → this is a **core decoder change** (add an empty-calldata arm to the route). That is bigger than a lido manifest; do an `ExitPlanMode` design note first and treat it as a focused decoder task, not lido scope-creep.
- **Files:** `crates/policy-engine-wasm/src/declarative_exports.rs`, `registryV2/scripts/build-index.ts`, `registryV2/manifests/lido/{steth,wsteth}/`, `data/golden/v3-decode/lido/corpus.json`.
- **Gate:** a real bare-ETH-to-stETH and bare-ETH-to-wstETH tx in the corpus decodes to `liquid_staking.stake` (not warn); `check:surface` still PASS (the sentinel isn't an ABI selector — confirm it doesn't break I1); `--require-expect-body` pins the value.

### L2 — `*WithPermit` embedded allowance grant  ✅ RESOLVED (was MEDIUM)
> **Resolution (this run, axis-2 ActionBody extension):**
> - **ActionBody:** new `EmbeddedPermit { value, deadline }` struct + optional
>   `embedded_permit: Option<EmbeddedPermit>` on `RequestWithdrawalAction`
>   (`action/src/liquid_staking/request_withdrawal.rs`). `#[serde(default,
>   skip_serializing_if)]` → absent for the non-permit variants. The signature
>   (`v,r,s`) stays decode-only.
> - **Cedar:** `LiquidStaking::EmbeddedPermit` type + optional `embedded_permit?`
>   attribute on `RequestWithdrawalContext` (`request_withdrawal.cedarschema`);
>   lowering inserts it when present (`lowering_v2/.../request_withdrawal.rs`). New
>   `request_withdrawal_with_permit_conforms` conformance test.
> - **Manifests:** the 2 `*WithPermit` manifests emit `embedded_permit` from the
>   decoded permit tuple. **Decode gotcha caught:** calldata tuples decode to a
>   **positional array**, so the path is `$args._permit[0]` (value) /
>   `$args._permit[1]` (deadline) — NOT named `.value`/`.deadline` (named access
>   only works for typed-data, not calldata; validate flagged it).
> - **Corpus:** the 2 `*WithPermit` entries pin `embedded_permit.value` +
>   `.deadline`, independently verified against the real calldata.
>
> **Gates green:** `cargo test --workspace` 1378/0 (incl. the new conformance test);
> `validate --filter lido` 12 OK; `corpus --filter lido --require-expect-body`
> 11/11; check:surface/manifest/tokens PASS; wasm-build + tsc clean; extension
> vitest 394/0; changed-crate clippy/fmt clean.
>
> **Note:** the queue (spender) is implicit — it is the `to` contract itself, so no
> spender field is needed; the `value`/`deadline` are the policy-relevant fields.
>
> ---
>
> - **What:** `requestWithdrawalsWithPermit` / `requestWithdrawalsWstETHWithPermit` carry an in-calldata EIP-2612 permit `(value, deadline, v, r, s)` that grants the WithdrawalQueue an allowance over the user's stETH/wstETH. The current body emits only `request_withdrawal{amounts, owner, token}` and **drops** the permit's `value`/`deadline`.
- **Why it matters:** ScopeBall is a *permission*-scope analyzer; an embedded allowance grant is on-mission to surface. Currently invisible.
- **Materiality:** bounded — `spender` is the queue contract itself (self-contained), and the withdrawal `amounts` (the economic intent) ARE surfaced. So low risk, but a real modeling omission.
- **Approach:** axis-2 Tier-3 extension — add an optional `embedded_permit: { value, deadline }` (spender is implicit = the queue) to `RequestWithdrawalAction` (`crates/policy-server/asset-model/action/src/liquid_staking/request_withdrawal.rs`). Then map it in the two `*WithPermit` manifests' `emit.body` (`$args._permit.value`, `$args._permit.deadline`). Requires the full Tier-3 downstream: effect/view/sync if needed, `lowering_v2/liquid_staking/request_withdrawal.rs`, cedarschema, conformance (`MissingAction`). Non-permit request variants leave the field absent.
- **Files:** `.../liquid_staking/request_withdrawal.rs`, `crates/policy-engine/src/lowering_v2/liquid_staking/request_withdrawal.rs`, `schema/policy-schema/actions/liquid_staking/**`, the 2 `*WithPermit` manifests, corpus expect_body.
- **Gate:** `cargo test --workspace` (conformance) green; the 2 corpus entries' expect_body pin `embedded_permit.value`; wasm-build + clippy/fmt (ActionBody changed).

### L3 — corpus `expect_body` independence  ✅ RESOLVED (was MEDIUM)
> **Resolution (this run):** the 5 not-independently-derived intents were
> independently `cast calldata-decode`'d (off-agent), 5/5 CONFIRM, zero mismatch
> (selectors cross-checked via `cast sig`; amounts round-tripped dec↔hex; the
> selector→token inference for the request* variants validated). Three new
> reasoned-from-calldata hand goldens added to `tests/v3_decode_harness.rs` —
> `lido_unwrap_amount_and_live_input_decode`, `lido_claim_withdrawal_decodes_request_id`,
> `lido_request_withdrawals_wsteth_decodes_wsteth_token_and_owner` — so the
> independent-golden set is now **7** (submit, requestWithdrawals, wrap×2, unwrap,
> claimWithdrawal, requestWithdrawalsWstETH). The remaining corpus entries
> (claimWithdrawals, the two `…WithPermit`, the two native-transfer stakes) are
> documented as `cast`-cross-checked in the corpus `_comment`. The §9.4
> self-reference is removed. Gate: 7/7 lido goldens pass; `corpus --require-expect-body`
> 11/11.
>
> ---
>
> - **What:** the 9 corpus `expect_body` values were taken from the decoder's own output. 4 intents (submit, requestWithdrawals, wrap×2) have independent hand goldens in `v3_decode_harness.rs` (reasoned from calldata); the other 5 (unwrap, claimWithdrawal, claimWithdrawals, requestWithdrawalsWstETH, requestWithdrawalsWstETHWithPermit) are pinned but **not independently derived** → they verify regression, not current correctness.
- **Why it matters:** removes residual circularity in the §9.4 semantic pin.
- **Materiality:** low-medium (test rigor, not a runtime bug).
- **Approach:** for each of the 5, independently ABI-decode the corpus tx's calldata (`cast --calldata-decode` / `cast abi-decode`) and confirm the pinned values are the true decoded values; record the derivation in a comment. Optionally add hand goldens for unwrap + claim + requestWstETH to `v3_decode_harness.rs`.
- **Files:** `data/golden/v3-decode/lido/corpus.json` (comments), `crates/integration-tests/tests/v3_decode_harness.rs` (new goldens).
- **Gate:** new goldens pass; `--require-expect-body` still 9/9.

### L4 — transferShares / transferSharesFrom real-tx coverage  ✅ RESOLVED (was LOW)
> **Resolution (this run):** a deeper Etherscan sweep (~block 11M→25.2M, ~1.68M
> top-level stETH txs) FOUND both (the recent 30k window had 0 because they thin
> out in the contract's earlier history): **transferShares 85**, **transferSharesFrom
> 15**. Two real-tx corpus entries added with field-level pins reasoned from the
> calldata: `transferShares 0xab81357c…` (recipient `0x855eaff5…`, shares
> `0x4538fb9513ff97` = 19_484_426_582_097_815) and `transferSharesFrom 0x71b67f44…`
> (sender `0xdb4ef21a…`, recipient `0x0b70f851…`, shares `0x105ef39b2000` =
> 18_000_000_000_000). `corpus --filter lido --require-expect-body` = **13/13**.
>
> ---
>
> - **What:** both are covered (manifests + synthetic), but the 30k recent sweep had **0** real txs for them, so they're real-tx-unverified.
- **Approach:** targeted Etherscan pull on stETH filtered to selectors `0x8fcb4e5b` / `0x6d780459` over a deeper block range; if found, add a corpus entry each (with expect_body). If genuinely absent on mainnet recently, record "accept: low-volume, synthetic+manifest covered" in evidence.
- **Files:** corpus, evidence.
- **Gate:** corpus grows or an explicit accept note is added.

### L5 — token-metadata semantic precision  ✅ RESOLVED (was LOW)
> **Resolution (this run):**
> - **`PegKind::ExchangeRate`** added (`token/kind.rs`) — "non-rebasing wrapper
>   redeemable for a monotonically rising amount of the underlying via an on-chain
>   exchange rate (wstETH→stETH, rETH→ETH); balance fixed, value accrues in the
>   rate". Purely additive (no exhaustive `match` on `PegKind` anywhere — verified
>   by grep; `PegKind` is not lowered to Cedar, so no cedarschema change). wstETH's
>   token JSON `peg_kind` flipped `hard_peg`→`exchange_rate`. A
>   `peg_kind_wire_names_round_trip` unit test locks the serde wire names the
>   registry JSON depends on (`exchange_rate` ↔ `ExchangeRate`).
> - **unstETH `token_kind` decision: KEEP `stake_receipt`.** `MaturityNote` was
>   considered but rejected — it requires a fixed `maturity: Time`, which a Lido
>   withdrawal NFT does NOT have (finalization depends on queue processing + oracle
>   reports, not a fixed timestamp). `stake_receipt` (a redemption/withdrawal claim
>   on the underlying) is the honest best-fit; no JSON change.
>
> **Gates:** `cargo test -p policy-state` (incl. round-trip) green; `check:tokens`
> 0 errors; build-index clean; wasm-build + changed-crate clippy/fmt clean.
>
> ---
>
> - **What:** `PegKind` enum = {HardPeg, SoftPeg, Rebasing} only; wstETH is an index-appreciating wrapper (not 1:1), so it uses the best-fit `hard_peg` — imprecise. unstETH NFT uses `stake_receipt` (a withdrawal/maturity claim; `maturity_note` would be apter but is unused).
- **Approach:** add a `PegKind` variant for index/exchange-rate wrappers (e.g. `ExchangeRate`) in `crates/policy-server/asset-model/state/src/token/kind.rs` — exhaustive-match edit across serialization + any lowering that matches PegKind — then set wstETH to it. Decide unstETH `token_kind` (keep stake_receipt vs introduce maturity_note usage). Re-run `check:tokens` + build-index.
- **Files:** `.../token/kind.rs`, `registryV2/tokens/1/0x7f39…json` (+ unstETH), wasm if the type is wasm-exported.
- **Gate:** `cargo test --workspace`, `check:tokens` PASS, build-index clean.

---

## Larger — real work, separate scope (get a user decision before starting)

### L6 — enrichment live-inputs never materialize at runtime  ⏳ (A) landed · (B) designed
> **(A) Lido slice ✅ landed** (`03de73e3`): the 3 Lido `decoder_id`s
> (`lido_wsteth_by_steth` / `lido_steth_by_wsteth` / `lido_pooled_eth_by_shares`)
> are now registered in `DecoderRegistry::with_builtins()` (`sync/.../decoder.rs`,
> aliasing `decode_u256_as_string`). Inert until the framework route exists, but
> ready — zero added risk.
>
> **(B) Framework design ✅ written**: full design doc at
> `crates/integration-tests/onboarding/lido/L6_ENRICHMENT_RPC_DESIGN.md` (route +
> handler + AppState injection + config + **security threat model** + impl steps +
> test plan + woojinnn coordination). **Key findings:** (1) the gap is a single
> missing hop — a server-side `POST /v1/rpc` fetch+decode route (`app.rs` has none;
> `/evaluate` stubs `results = {}`); (2) the v2 wire carries **no `to`/`data`** —
> `method` is an opaque name, `params` structured JSON — so the server **cannot**
> be a generic eth_call proxy and **must** whitelist-resolve each call against a
> registry call-spec mirror (this is both the function and the load-bearing SSRF
> control); (3) verdict-isolation is **not** structural — a policy that *gates* on a
> `context.custom.*` enrichment field WILL flip on a wrong value (`materialize_v2.rs:449-526`),
> so the whitelist is what keeps gating-consumed values trustworthy.
>
> **Still framework-wide + needs woojinnn (route tier/auth, middleware, config,
> provider isolation) before implementation.** (B) is design-only; not started.
>
> ---
>
> - wrap/unwrap/transferShares declare `onchain_view` live-inputs (getWstETHByStETH etc.), but the policy-RPC host that fetches them is **dormant framework-wide** (no configured endpoint; verdicts are 100% local WASM Cedar). So the user sees the input amount, never the converted output amount.
> - **Not a lido fix** — it's the whole `/v1/rpc` dormancy (see `reference_v2_enrichment_and_rpc_dormant`). Wiring it is a framework/infra task (woojinnn's policy-server).

### L7 — multichain (L2 wstETH) not covered
- Only Ethereum mainnet. L2 wstETH (Arbitrum/OP/Base) + Lido-on-other-chains are deferred by the single-representative-chain rule. Onboarding them = a **separate framework run** per chain. Additive scope; user decides if/when.

### (context, not a lido gap) router-routed staking
- Dune 7d: 18.4% of stETH `submit` is internal (1inch/Curve zaps/DeFi Saver/wstETH-receive). Those txs have `tx.to` = the integrator, so they are the integrators' surfaces (1inch already separately onboarded), not Lido's wallet-facing surface. Nothing to "fix" in lido; just don't let the 99.6% direct-coverage be misread as "all Lido staking".

---

## Structural — ScopeBall design choices (accept + document, do NOT fake a fix)

- **No simulation.** ScopeBall is static-decode only (per repo CLAUDE.md). It decodes intent ("submit X ETH"), not outcome (resulting balance, slippage, claimability). Accept.
- **No signature validation.** Permit message fields are decoded; the signature is not verified. The stETH domain `version="2"` gotcha matters to a validator, not to ScopeBall's decode. Accept (decode shows *what* you sign, not *whether the sig is valid*).
- **decode ≠ semantic correctness.** 99.6% "decode" = "produces a non-Unknown ActionBody", not "is semantically correct"; field correctness is spot-checked (corpus + goldens), not proven over all 30k. Inherent; mitigated by L3.
- **Scope is human judgment.** The gates check internal consistency, not scope correctness (a missed wallet-facing contract would still pass if internally consistent; I0 only cross-checks docs.lido.fi, which can itself omit). Inherent framework limit — keep challenging the contract inventory against Dune/Etherscan labels.

---

## How to close an item
1. Implement under the relevant Addressable item's Approach.
2. Run that item's Gate + the global land gates (`cargo test --workspace`, `v3-harness validate/corpus --require-expect-body --filter lido`, `check:surface/manifest/tokens`; wasm-build + clippy/fmt if Rust/schema changed).
3. Update **this file** (mark the item resolved) **and** `evidence.md` (P3/P4 rows + final claim).
4. `git add <exact files>` (no `-A`) + commit with `Co-Authored-By`. No push/merge unless the user asks.
