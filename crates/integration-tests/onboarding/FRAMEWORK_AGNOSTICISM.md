# Framework protocol-agnosticism — audit + refactor roadmap

> Audit of the onboarding framework for **protocol-specific hardcoding in generic
> code**: every place where onboarding a *new* protocol forces editing generic
> framework files instead of only that protocol's own artifacts
> (`manifests/` / `surface/` / `tokens/`, and Tier-3 domain/action files only when a
> genuinely new domain is introduced). Run 2026-06-03 (5-agent audit across the
> decode route, registry gates, decoder registry, harness/model).

## Verdict — **B+**

The framework is **agnostic where it matters most**: the strategy grammar
(`single_emit` / `opcode_stream_dispatch` / `array_emit` / `tagged_dispatch` /
`multicall_recurse`), placeholder substitution, bridge-key routing, ABI decode,
`build-index.ts` resolver dispatch, the harness fuzzer/oracle, the ActionBody/Cedar
exhaustive-match model, and the enrichment `decode_any` two-tier fallback are all
data-driven with **no protocol-name control flow**. The leaks are **localized
deployment-data tables baked into generic Rust/TS** + one gate correctness defect —
not architectural coupling.

## ✅ Landed this run (safe high-value wins)

| | Fix | Commit |
|---|---|---|
| **R1** | `build-index` now actually validates manifest `$fn` names against the mappers WHITELIST (was a **false claim** — 0 `$fn` refs in build-index; a typo'd `$fn` fail-closed at decode time in the wallet, not at build). Single source of truth `crates/adapters/mappers/src/declarative/fn_whitelist.json` + Rust sync test + build-index recursive `$fn` scan. | `e25843e6` |
| **R2** | Lido `onchain_view` manifests use the generic `decoder_id: "u256"`; removed the 3 `lido_*` aliases from the shared `decoder.rs` (they were redundant uint256 aliases — a leak introduced by L6(A)). Lido enrichment stays ready with **zero** Lido-specific decoder code. | `f92c09b3` |
| **R3** | Dropped the hand-maintained `VALID_DOMAINS` array in the harness oracle (a cross-crate restatement of the ActionBody domain enum — a drift trap). The L2 `Vec<Action>` round-trip already rejects unknown `domain` tags (strictly stronger). | `74b0a87e` |

Gates: `check:manifest` 1487 OK · `corpus` ALL **338/338** · `cargo test -p mappers`
48/0 · `-p policy-sync` 4/0 · integration-tests lib 18/0 · clippy/fmt clean ·
negative-tested R1 (a typo'd `$fn` now fails the build).

## ⏳ DEFER-DESIGN — real leaks needing a bigger (riskier) change

These touch the **core decode path** and/or **cross-protocol** surfaces, so they are
documented here with direction rather than refactored ad-hoc. **R4 is the highest
agnosticism win.**

### R4 — chain/address-keyed `$resolved` tables → per-bundle manifest constants  ⭐ highest agnosticism win
- **Leak:** `declarative_exports.rs` pre-populates static `$resolved` tables keyed by
  `(chain, target)` for protocol deployment data — Aave WrappedTokenGateway→POOL
  (`:764-780`), **Compound v3 Comet→base-asset (~30 deployments × 11 chains, `:782-871` — the single largest protocol-identity table)**, Uniswap V4 PoolManager-by-chain
  (`:492-504`), WETH-by-chain (`:472-483`, duplicated at `:996-1007`).
- **Why not agnostic:** every new Aave/Compound/V4 deployment forces editing the
  generic WASM route + rebuilding the bundle, even though the bundle is already keyed
  by `(chain, address)` and the value is immutable + known at manifest-authoring time.
- **Direction:** add a per-bundle `resolved_constants: Map<String,String>` (schema field,
  round-tripped through `validateEmitShape` + `declarative_install_v3_json`) that the
  route merges into `ctx.resolved` from the **already-in-hand matched bundle** — then
  migrate each table into its protocol's manifests and delete the Rust arm. **Migrate one
  protocol at a time**, guarded by that protocol's golden/real-tx fixtures (the derivation
  math is untouched → byte-equivalence provable); do **not** batch. WETH stays a shared
  chain-constants helper (chain fact, not protocol fact). *Does NOT move any keccak/unpack
  derivation — only the source of static constants.*
- **Highest-payoff single step:** Compound base-asset (`:782-871`) — kills the largest
  table + the "rebuild WASM per new Comet" tax. **Risk:** medium (hot `ctx.resolved` path);
  bounded by per-protocol golden re-verification + WASM rebuild.

### Other deferred leaks (direction only)
- **`return_signature` for enrichment decode** — `decoder_id` must be pre-registered in
  Rust (`decoder.rs` / `abi_decoder/types.rs`). Direction: add optional
  `return_signature` to `DataSource::OnchainView`; `decode_any` decodes generically via
  `DynSolType::from_str` when present → **zero-Rust** enrichment onboarding for any return
  shape. This makes R2's lesson permanent (reserve `decoder_id` only for packed/reshape
  overrides). Sync-layer schema change.
- **UniswapX reactor address table** (`builtin_fn.rs:259-287`, duplicated in the fuzzer
  `single_emit.rs:44-200`) — pass the order family / decode-signature as a `$fn` arg from
  the manifest (the reactor is already the routing key); remove the address table.
- **morpho_market_id / uniswap_v3_path unconditional-inject** (`declarative_exports.rs:529-530`,
  `:2197-2283`) — convert the route-level field-name probe to opt-in `$fn` calls
  (`$fn morpho_market_id [$args.marketParams]`), unifying with the existing `$fn` pattern.
  Keccak/unpack code stays (inherent); only the **dispatch** moves into manifest data.
- **V4 pool_id + opcode-stream swap-param normalization** (`:1378-1489`, `:2057-2168`) —
  highest effort, hot opcode path, post-#497 variable-arity. Per-opcode `derive` directive.

## ✓ ACCEPT-INHERENT — looks non-agnostic, is by-design (do NOT over-engineer)

- **`$fn` WHITELIST mechanism.** A call-form DSL deliberately cannot run arbitrary code;
  a genuinely new computation (Curve variable-hop output token, signed `amountSpecified`)
  *requires* Rust. The whitelist is the disciplined, audited seam — that's the feature.
  (Only R1's false *mirror claim* needed fixing, not the mechanism.)
- **`decode_permit2_allowance` / `decode_aave_user_data`.** Packed sub-word fields
  (uint160/uint48) and named struct reshapes are not expressible as a flat ABI return.
  The hand-coded mapper is the correct home — reserve `DecoderRegistry` for exactly these.
- **Permit2 `validateTypedDataShape` guard.** Guards a real Permit2-specific routing
  collision (`PermitWitnessTransferFrom`); Permit2 is the only known case.
- **ActionBody domain exhaustive-match + Cedar registration arrays + `assert_conforms`.**
  This is the *intended* inherent cost of adding a **new domain** (axis 1): exhaustiveness
  + conformance convert silent gaps into loud build/test failures. A new domain *is* new
  framework capability — do not try to make it zero-Rust. (Doc nit: the extension guide
  says "3 Cedar registration sites"; the real count is **4** generic arrays —
  `SHIPPED_SCHEMA_FILES`, `ACTION_CONTEXT_TYPES`, `REGISTERED_ACTIONS`, `RESOLVER_TABLE`.)

## Recommended next

**R4 / Compound base-asset → manifest `resolved_constants`** is the highest-leverage
*agnosticism* refactor (deletes the largest protocol-identity table + the per-deployment
WASM-rebuild tax). It is a focused, single-protocol-at-a-time change on the core decode
path — best done as its own session with the Compound golden/real-tx fixtures as the guard
and `./scripts/wasm-build.sh` + extension typecheck to confirm the bundle still loads.
