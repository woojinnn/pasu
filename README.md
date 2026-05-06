# policy-engine — v0.x reference implementation

A web3 wallet transaction policy engine. v0.1 wired up the smallest possible
end-to-end pipeline so the design choices in
`docs/specs/2026-05-05-policy-engine-design.md` could be validated against
real calldata. v0.x follow-on work has now landed the per-tx evaluation pass
(spec §4.5), the host-capability seam (`Oracle` + `Portfolio` + `Approvals` +
`StatWindows`), the **reserve-first reservation lifecycle** with post-this-tx
`windowStats` projection (spec §4.6), per-match request-origin attribution,
and a simplified `Adapter` trait that only produces actions + metadata —
sequencing now lives entirely in the `Pipeline`.

The codebase is a **Cargo workspace** split along the boundaries the design
document calls out: pipeline runtime, adapter SDK, individual adapter crates,
and an aggregator that exposes them as a "default registry".

## Workspace layout

```
policy-engine/                        # workspace root (virtual)
├── Cargo.toml                        # [workspace] members + shared dep versions
├── docs/                             # design spec
├── policies/                         # *.cedar / *.json policy artifacts, organized by action kind
│   ├── swap/                         #   policies that target Action::"swap" (per-leaf)
│   │   ├── max-swap-usd-100.cedar            (deny: input USD > 100, oracle-aware)
│   │   ├── max-swap-fee-bps-100.cedar         (deny: feeBips > 100)
│   │   ├── uniswap-only-allowlist.cedar       (deny: protocol not in allowlist)
│   │   ├── no-zero-min-output.cedar           (warn: minOutputAmount.raw == "0")
│   │   ├── min-output-usd-floor.cedar         (deny: minOutput USD < 10, oracle-aware)
│   │   ├── max-fraction-of-balance-2000-bps.cedar (deny: input > 20% of actor balance — Portfolio)
│   │   └── allowance-must-cover-input.cedar   (warn: allowance < input — Approvals)
│   └── tx/                           #   policies that target Action::"send_tx" (per-transaction)
│       ├── tx-blocklist.cedar                 (deny: target ∈ blocklist)
│       ├── tx-total-input-usd-cap-500.cedar   (deny: aggregated input USD > $500)
│       └── tx-window-swap-volume-usd-24h-cap-5000.cedar
│                                              (deny: 24h cumulative swap USD > $5000 — StatWindows)
│
└── crates/
    ├── policy-engine/                # ① runtime — split into focused modules
    │     core.rs            Address, Token, TransactionRequest, Action, AmountSpec, UsdValuation
    │     oracle.rs          Oracle trait + MockOracle
    │     portfolio.rs       Portfolio trait + MockPortfolio (current actor balances)
    │     approvals.rs       Approvals trait + MockApprovals (current ERC-20 allowances)
    │     stat_windows.rs    StatWindows trait + MockStatWindows
    │                        + reserve / settle / release lifecycle
    │     host.rs            HostCapabilities (oracle + optional portfolio/approvals/stats)
    │                        + builder
    │     policy.rs          PolicyEngine, PolicyEngineBuilder, PolicyRequest, Verdict,
    │                        MatchedPolicy, RequestKind { Leaf{index}, Tx }
    │     adapter.rs         Adapter trait (build_actions + leaf_metadata) + AdapterId
    │                        + AdapterError + MatchKey
    │     registry.rs        AdapterRegistry trait + ResolverOutcome
    │                        + AdapterIndex + MockAdapterRegistry
    │     context_keys.rs    Cedar context-field name constants (used by lowering)
    │     lowering/          ←── directory module
    │       mod.rs           module-level docs + curated re-exports
    │       decimal.rs       fixed-width decimal arithmetic helpers
    │       request.rs       request_from_action, request_for_tx,
    │                        requests_from_action[s], action_entities/context, amount_json
    │       enrich.rs        enrich_with_usd, enrich_actions_with_usd,
    │                        enrich_request_with_capabilities (+ stamp_portfolio_fields,
    │                        stamp_approval_fields), enrich_tx_request_with_window_stats,
    │                        compute_swap_window_deltas
    │     pipeline.rs        Pipeline orchestrator + LoweredRequests + evaluate /
    │                        evaluate_with_reservation + EvaluationOutcome
    │     prelude.rs         curated import surface for adapter authors
    │                        (`use policy_engine::prelude::*;`)
    │
    ├── adapters/                     # ② directory of internal adapter crates (one crate per protocol)
    │   ├── uniswap-v3/               # Uniswap V3 SwapRouter (4 swap functions + multicall)
    │   │   src/
    │   │     lib.rs                  ← module index + flat re-exports
    │   │     common.rs               ← shared: SWAP_ROUTER_MAINNET, TokenLookup,
    │   │                               shift_decimals, decode_v3_path, DecodeError
    │   │     exact_input_single.rs   ← single-hop, exact-in
    │   │     exact_input.rs          ← multi-hop, exact-in (packed bytes path)
    │   │     exact_output_single.rs  ← single-hop, exact-out
    │   │     exact_output.rs         ← multi-hop, exact-out (reversed path)
    │   │     multicall.rs            ← multicall expansion into leaf swaps
    │   │   tests/abi_cross_check.rs   8 sol!-byte-equivalence tests
    │   └── uniswap-v2/               # Uniswap V2 Router02 (6 functions)
    │       src/
    │         lib.rs                  ← module index + flat re-exports
    │         common.rs               ← shared: ROUTER, TokenLookup, native_eth, …
    │         swap_exact_tokens_for_tokens.rs
    │         swap_tokens_for_exact_tokens.rs
    │         swap_exact_eth_for_tokens.rs        (payable, ETH→Token)
    │         swap_eth_for_exact_tokens.rs        (payable, ETH→Token, exact-out)
    │         swap_exact_tokens_for_eth.rs        (Token→ETH)
    │         swap_tokens_for_exact_eth.rs        (Token→ETH, exact-out)
    │   └── universal-router/         # Universal Router command stream (V2/V3/V4 swaps)
    │
    ├── adapters-bundle/              # virtual registry — aggregates every adapter into a default registry
    │     lib.rs         pub fn default_registry() -> MockAdapterRegistry
    │     examples/e2e_swap.rs        runnable demo
    │
    └── integration-tests/            # workspace-level e2e tests
          tests/e2e_swap.rs                    11 end-to-end scenarios
          tests/adapter_into_request.rs        11 default + custom-override paths
          tests/extra_swap_policies.rs         14 extra leaf policies × happy/sad
          tests/composite_routers.rs            3 V3 multicall + UR composition
          tests/tx_pass.rs                      6 send_tx pass + RequestKind origin
          tests/capability_swap_policies.rs     7 Portfolio + Approvals enrichment
          tests/window_stats.rs                12 StatWindows reserve/settle/release +
                                                  reserve-first cap correctness
```

## Dependency DAG (no cycles)

```
            ┌─── adapters/uniswap-v3 ───┐
            │                            │
policy-engine ◄                            ◄── adapters-bundle
            │                            │             ▲
            └─── adapters/uniswap-v2 ───┘             │
                                                       │
                                  integration-tests ───┘
```

Each adapter crate depends on `policy-engine` for the `Adapter` trait + types
(via `policy_engine::prelude`). `adapters-bundle` aggregates every adapter
into a `default_registry()` so that downstream code never has to depend on
each adapter individually.

Adding a new internal adapter is a three-step:

1. Create `crates/adapters/<name>/` and add it to `[workspace] members`.
2. Add it as a dependency of `crates/adapters-bundle` and call `.with_adapter(...)`
   in `default_registry`.
3. Add fixtures and integration tests under `crates/integration-tests/tests/`
   if needed.

## What works in v0.x

- **13 swap/composite adapters** spanning three crates:
  - **Uniswap V3 SwapRouter** (`0xE592427A0AEce92De3Edee1F18E0157C05861564`):
    `exactInputSingle`, `exactInput`, `exactOutputSingle`, `exactOutput`,
    plus `multicall(bytes[])` and `multicall(uint256,bytes[])`
    (composite — recursively decoded leaf swaps).
  - **Uniswap V2 Router02** (`0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D`):
    `swapExactTokensForTokens`, `swapTokensForExactTokens`,
    `swapExactETHForTokens`, `swapETHForExactTokens`,
    `swapExactTokensForETH`, `swapTokensForExactETH`.
  - **Uniswap Universal Router**: command-stream `execute(...)` — extracts
    V2/V3/V4 swap commands into leaf `SwapAction`s so the existing swap
    policies apply unchanged. v4 hooks / sub-plans / Permit2 surfaces are
    flagged in context for policy-side defense.
  All wired up by `policy-engine-adapters-bundle::default_registry()`.
- **Per-action + per-tx evaluation passes** (spec §4.5). Every transaction
  emits one or more leaf `PolicyRequest`s (`action == Action::"<kind>"`,
  resource = `Protocol::"…"`) **plus exactly one** transaction-level
  request (`action == Action::"send_tx"`, resource = `Address_::"<to>"`)
  whose context carries `childCount`, `kinds`, `protocolsUsed`,
  `totalInputUsd`, `distinctRecipients`, `hasApprove`, `hasUnknown`,
  `allowRevertCount`, and host snapshots (see capabilities below). Both
  passes are aggregated under deny-overrides + warn-union; each
  `MatchedPolicy` carries an `origin: RequestKind { Leaf{index} | Tx }`
  so the host UI can attribute fired policies precisely.
- **`HostCapabilities`** — value-object seam between host and engine:
  ```rust
  HostCapabilities::new(&oracle)                              // oracle-only
  HostCapabilities::builder(&oracle)
      .with_portfolio(&pf).with_approvals(&ap).with_stats(&w).build()
  ```
  - **`Oracle`** — token → USD valuation. `MockOracle` for tests/playground;
    HTTP-backed impls slot into `oracle.rs` next to it.
  - **`Portfolio`** — `(owner, token) → AmountSpec`. Lowering stamps
    `actorBalanceInputToken` and a precomputed `inputFractionOfBalanceBps`
    so policies can express "swap input ≤ 20 % of balance" without Cedar
    decimal multiplication.
  - **`Approvals`** — `(owner, token, spender) → AmountSpec`. Lowering
    stamps `currentAllowance` (skipped for native ETH inputs) and a
    boolean `allowanceCoversInput` so policies can warn "approve required".
  - **`StatWindows`** — `(owner, keys) → snapshot { Decimal | Count }`,
    plus `reserve / settle / release` lifecycle. Snapshots include any
    active reservations. **Reserve-first model** in
    `Pipeline::evaluate_with_reservation`: deltas are reserved BEFORE
    the snapshot is taken, so `windowStats` is the **post-this-tx
    projection** — a 4900 + 200 swap fires a 5000 cap, two concurrent
    3000 swaps see each other's reservation, and `Verdict::Fail` rolls
    back the speculative reserve before returning. Plain
    `Pipeline::evaluate` (no side-effects) achieves the same projection
    by passing computed deltas through to the enricher without
    touching the stats backing store. Returns
    `EvaluationOutcome { verdict, reservation }` so the host can
    settle or release based on whether the user signs and the tx
    confirms.
  - **Fail-open everywhere**: a missing capability (or a missing per-key
    record) omits the field; policies guard with `context has "field"`.
- **9 shipped policies** across two leaf-vs-tx directories:
  - `policies/swap/`: `max-swap-usd-100` (deny),
    `max-swap-fee-bps-100`, `uniswap-only-allowlist`, `no-zero-min-output`,
    `min-output-usd-floor`, `max-fraction-of-balance-2000-bps`,
    `allowance-must-cover-input`.
  - `policies/tx/`: `tx-blocklist`, `tx-total-input-usd-cap-500`,
    `tx-window-swap-volume-usd-24h-cap-5000`.
- **Mock adapter registry**: in-memory; strict `(chain, to, selector)`
  exact-match; surfaces `NoMatch` / `Resolved` / `Ambiguous`.
- **Cedar evaluator**: `cedar-policy` 4.x; `@severity` annotation drives the
  tri-state verdict; default-allow is enforced via a baseline permit.
- **Simplified `Adapter` trait**: adapters produce
  `build_actions(tx) -> Vec<Action>` plus optional
  `leaf_metadata(tx, leaves) -> Vec<Map<String, Value>>` (default empty
  per leaf). Sequencing — USD enrichment → request building → capability
  enrichment → metadata merge → tx-level summary → window stats — runs
  entirely in `Pipeline`. The previous `into_request[s]` /
  `lower_requests` overrides are gone; protocol-specific data (e.g.
  Universal Router `allowRevert`, V4 hook flags) reaches the request
  context through `leaf_metadata`. Pipeline hard-fails with
  `PipelineError::AdapterBuild` if `metas.len() != leaves.len()` (closes
  a release-mode policy bypass surface — `debug_assert` alone was
  insufficient).
- **Cedar context-field constants** (`policy_engine::context_keys`):
  every field name produced by lowering is a `pub const`, so a typo on
  the engine side surfaces at compile time. Cedar policy files keep
  their string literals — the contract is "the policy literal matches
  the constant value here".

## Running

```bash
# Run the full test suite (174 tests + 1 ignored doctest across the workspace).
cargo test --workspace
cargo test --workspace --release   # release-mode tests catch metadata-mismatch bypass

# Run the demo.
cargo run -p policy-engine-adapters-bundle --example e2e_swap
```

Expected example output:

```
─── 50 USDT (under cap) ───   decision : Allow
─── 100 USDT (at cap) ───      decision : Allow
─── 200 USDT (over cap) ───    decision : Deny
                                matched  : user/max-swap-usd-100 (Deny)
                                           USD value of swap input exceeds 100
```

## ABI handling

Encoding and decoding is delegated to alloy's `sol!` macro inside
`crates/adapters/uniswap-v3/src/decode.rs`. The macro derives both the
encoder and decoder from the literal Solidity signature, so we don't ship
hand-rolled byte-shuffling. The `tests/abi_cross_check.rs` file maintains a
parallel `sol!` definition and asserts byte-equivalence — drift is caught
mechanically.

## Trait taxonomy

| Concept | Where | What |
|---|---|---|
| **Decode** (parse bytes → typed values) | `adapters/uniswap-v3/src/exact_input_single.rs` (one file per Uniswap function) | `decode` / `encode` via `sol!` |
| **Adapter** (typed values → semantic actions) | `policy-engine/src/adapter.rs` (trait) + `adapters/*/` (impls) | `Adapter::build_actions` → one or more semantic `Action`s; `Adapter::leaf_metadata` → optional protocol-specific data per leaf (e.g. UR `allowRevert`) |
| **Pipeline** (sequencing + enrichment) | `policy-engine/src/pipeline.rs` | resolves the adapter, walks the lowering chain, runs reserve-first window-stats projection, calls Cedar |
| **PolicyEngine** (Cedar evaluation) | `policy-engine/src/policy.rs` | consumes origin-tagged `PolicyRequest`s, returns aggregated `Verdict` |

## Verdict shape

```rust
pub enum Verdict {
    /// No matched forbid → transaction passes.
    Pass,
    /// Only `@severity("warn")` policies fired. Wallet should display
    /// reasons and require explicit user confirmation before signing.
    Warn(Vec<MatchedPolicy>),
    /// At least one `@severity("deny")` policy fired (deny-overrides). Wallet
    /// must refuse to sign. The vec carries BOTH the deny entries AND any
    /// also-fired warn entries — no info is dropped (warn-union).
    Fail(Vec<MatchedPolicy>),
}

pub struct MatchedPolicy {
    pub policy_id: String,
    pub reason:    Option<String>,
    pub severity:  Severity,    // Deny or Warn — preserved per-element
    pub origin:    RequestKind, // Leaf { index } | Tx — which request fired this match
}

pub enum RequestKind {
    Leaf { index: usize },  // 0-based index into the leaf request list
    Tx,                     // the per-transaction send_tx pass
}
```

The variant tells the host **what to do** (deny-overrides is enforced by the
engine, not the host). `MatchedPolicy.severity` lets the host generically
iterate `verdict.matched()` and render warnings vs errors distinctly even
when both kinds fired. `MatchedPolicy.origin` lets the UI attribute the
match to a specific leaf (e.g. "leaf #2 in this multicall") or to the
transaction-level pass. Convenience methods on `Verdict`: `is_failure`,
`has_warnings`, `matched() -> &[MatchedPolicy]`.

## Test coverage (174 total + 1 ignored doctest, debug + release)

| Crate / file | Tests | What |
|---|---:|---|
| `policy-engine/src/*` (unit) | ~42 | core types, oracle/policy/registry/lowering invariants, RequestKind plumbing, Portfolio + Approvals + StatWindows mocks (snapshot / reserve / settle / release) |
| `policy-engine-adapter-uniswap-v3/src/*` (unit) | 33 | Per-function encode/decode, selector pins, multicall expansion, V3 packed-path decoding |
| `policy-engine-adapter-uniswap-v3/tests/abi_cross_check.rs` | 8 | Hand-rolled bytes vs `sol!` macro byte-equivalence |
| `policy-engine-adapter-uniswap-v2/src/*` (unit, 7 modules) | 23 | Per-function: round-trip / selector / build (native-ETH + amount-cap) |
| `policy-engine-adapter-universal-router/src/*` (unit) | 4 | execute selector pins, ABI round-trip, V3/V4 command expansion |
| `integration-tests/tests/e2e_swap.rs` | 11 | USDT/USDC/WETH inputs at boundaries, stale/missing oracle, unknown target, corrupt calldata |
| `integration-tests/tests/composite_routers.rs` | 3 | Max-swap policy denies leaf swaps inside V3 multicall and Universal Router V3/V4 commands |
| `integration-tests/tests/adapter_into_request.rs` | 11 | Default lowering chain; custom-override behavior via `leaf_metadata`; `Pipeline` over `&dyn AdapterRegistry`; custom `AdapterRegistry`; **`leaf_metadata` length mismatch (short and long) hard-fails** in both debug and release |
| `integration-tests/tests/extra_swap_policies.rs` | 14 | 4 new leaf policies × happy/sad, oracle-missing skip, deny-overrides-preserves-warn composition |
| `integration-tests/tests/tx_pass.rs` | 6 | `send_tx` per-tx pass: single-swap shape, multicall aggregation cap, leaf-deny + tx-warn distinct origins, NoMatch, pure-ETH transfer with blocklist, Universal Router `allowRevertCount` propagation |
| `integration-tests/tests/capability_swap_policies.rs` | 7 | Portfolio + Approvals enrichment: balance-fraction cap fires/passes, fail-open without capability, allowance warn fires/passes, native-ETH skip, multicall propagation per-leaf |
| `integration-tests/tests/window_stats.rs` | 12 | StatWindows reservation lifecycle (snapshot reflects confirmed + reservations, settle promotes, release rolls back) **and reserve-first cap correctness**: 4900+200 boundary fires, two sequential 3000 reserved evals (second sees first's reservation), `evaluate` and `evaluate_with_reservation` agree on projected state, fail-open without capability, `Fail` releases speculative reservation |
| **Total** | **174** | |

## What's deliberately not here yet

- **ERC-20 `approve` / `transfer` adapters** — the highest-value remaining
  v0.x gap. The `Approvals` capability is in place, so once the adapter
  lands, "unlimited approve" / spender-allowlist policies (spec §6.3)
  apply immediately.
- 1inch, CowSwap, Curve, Balancer, Pendle adapters
- Real-API capability implementations — HTTP/RPC-backed `Oracle`,
  `Portfolio`, `Approvals`, and `StatWindows` impls slot in next to the
  `Mock*` types
- Time-decay for `StatWindows` — the in-memory `MockStatWindows` keeps
  every settled delta forever; production impls would timestamp settled
  entries and prune outside their window
- Real concurrent-stress validation of the reserve-first model — the
  shipped tests cover sequential ordering and post-this-tx projection,
  not multi-thread races. A `loom`-style stress test is a future add
- Capability-file de-duplication via macro / generic — three Mock impls
  share a common `Trait + Mock<HashMap> + with_X + Error` shape;
  intentionally deferred until a 4th capability arrives so the
  cost/benefit is data-driven
- WASM compute step for adapters that can't decode declaratively
- Manifest-driven adapters (still hardcoded in Rust)
- Marketplace, packaging, signing
- Playground UI
- Cedar Symbolic Compiler integration (publish-time + install-time
  conflict detection — see spec §8.6)

Those are the next steps, sequenced in the design doc's §11 "Implementation
roadmap (build order — playground-first)".
