# policy-engine — v0.1 MVP

A web3 wallet transaction policy engine. v0.1 wires up the smallest possible
end-to-end pipeline so the design choices in
`docs/specs/2026-05-05-policy-engine-design.md` can be validated against real
calldata.

The codebase is a **Cargo workspace** split along the boundaries the design
document calls out: pipeline runtime, adapter SDK, individual adapter crates,
and an aggregator that exposes them as a "default registry".

## Workspace layout

```
policy-engine/                        # workspace root (virtual)
├── Cargo.toml                        # [workspace] members + shared dep versions
├── docs/                             # design spec
├── policies/                         # *.cedar / *.json policy artifacts, organized by action kind
│   └── swap/                         #   policies that target Op::"swap"
│       ├── max-swap-usd-100.cedar / .json   (deny: input USD > 100, oracle-aware)
│       ├── max-swap-fee-bps-100.cedar       (deny: feeBips > 100)
│       ├── uniswap-only-allowlist.cedar     (deny: protocol not in allowlist)
│       ├── no-zero-min-output.cedar         (warn: minOutputAmount.raw == "0")
│       └── min-output-usd-floor.cedar       (deny: minOutput USD < 10, oracle-aware)
│
└── crates/
    ├── policy-engine/                # ① runtime — split into focused modules
    │     core.rs        Address, Token, TransactionRequest, Action, AmountSpec, UsdValuation
    │     oracle.rs      Oracle trait + MockOracle (HTTP-backed impls slot in here later)
    │     policy.rs      PolicyEngine, PolicyEngineBuilder, PolicyRequest, Verdict
    │     adapter.rs     Adapter trait + AdapterId + AdapterError + MatchKey  (~85 lines)
    │     registry.rs    AdapterRegistry trait + ResolverOutcome
    │                    + AdapterIndex + MockAdapterRegistry + tests          (~240 lines)
    │     lowering.rs    enrich_with_usd + request_from_action
    │                    + decimal arithmetic helpers + tests                  (~210 lines)
    │     pipeline.rs    Pipeline orchestrator (generic over `R: AdapterRegistry`)
    │     prelude.rs     curated import surface for adapter authors
    │                    (`use policy_engine::prelude::*;`)
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
          tests/e2e_swap.rs           11 end-to-end scenarios
          tests/policy_json.rs        10 CedarJSON ↔ text parity tests
          tests/adapter_into_request.rs   7 Adapter::into_request flavor tests
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

## What works in v0.1

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
- **5 swap policies** under `policies/swap/`:
  - `max-swap-usd-100` (deny / authored both as Cedar text + CedarJSON)
  - `max-swap-fee-bps-100` (deny when feeBips > 100)
  - `uniswap-only-allowlist` (deny non-allowlisted protocols)
  - `no-zero-min-output` (warn on `minOutputAmount.raw == "0"`)
  - `min-output-usd-floor` (deny when minOutput USD < 10, oracle-aware)
- **Mock adapter registry**: in-memory; strict `(chain, to, selector)`
  exact-match; surfaces `NoMatch` / `Resolved` / `Ambiguous`.
- **Mock oracle**: in-memory price table for USDT, USDC, WETH.
- **Cedar evaluator**: `cedar-policy` 4.x; `@severity` annotation drives the
  tri-state verdict; default-allow is enforced via a baseline permit.
- **`Adapter::into_request`**: one-shot `calldata → PolicyRequest` method on
  the `Adapter` trait, default-implemented as `build → enrich_with_usd →
  request_from_action`. Override it to bypass the `Action` intermediate.

## Running

```bash
# Run the full test suite (143 tests across the workspace).
cargo test --workspace

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
| **Adapter** (typed values → policy-evaluable form) | `policy-engine/src/adapter.rs` (trait) + `adapters/*/` (impls) | `Adapter::build` → semantic `Action`; `Adapter::into_requests` → one or more Cedar `PolicyRequest`s |
| **PolicyEngine** (Cedar evaluation) | `policy-engine/src/policy.rs` | consumes one or more `PolicyRequest`s, returns aggregated `Verdict` |

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
    pub severity:  Severity,   // Deny or Warn — preserved per-element
}
```

The variant tells the host **what to do** (deny-overrides is enforced by the
engine, not the host). `MatchedPolicy.severity` lets the host generically
iterate `verdict.matched()` and render warnings vs errors distinctly even
when both kinds fired. Convenience methods on `Verdict`: `is_failure`,
`has_warnings`, `matched() -> &[MatchedPolicy]`.

## Test coverage (143 total)

| Crate / file | Tests | What |
|---|---:|---|
| `policy-engine/src/core.rs` (unit) | 7 | Address normalization, token keys, selector slicing, optional `gas`/`nonce` |
| `policy-engine/src/oracle.rs` (unit) | 4 | Mock price lookup, error on miss, chain-id keying |
| `policy-engine/src/policy.rs` (unit) | 6 | Cedar wrapper: pass/warn/fail variants, fail-overrides-warn, bad-severity rejection |
| `policy-engine/src/registry.rs` (unit) | 6 | Registry resolution edges (single match, no-match by chain/selector/target, ambiguous, empty) |
| `policy-engine/src/lowering.rs` (unit) | 5 | Decimal arithmetic helpers used by USD valuation |
| `policy-engine-adapter-uniswap-v3/src/common.rs` (unit) | 8 | `shift_decimals` cases, `TokenLookup`, V3 packed-path decoding |
| `policy-engine-adapter-uniswap-v3/src/exact_input_single.rs` (unit) | 12 | encode/decode round-trip; selector pin; bad-data; `match_keys`; `build` paths |
| `policy-engine-adapter-uniswap-v3/src/exact_input.rs` (unit) | 4 | round-trip, selector pin, build path, multi-hop fee average |
| `policy-engine-adapter-uniswap-v3/src/exact_output_single.rs` (unit) | 3 | round-trip, selector pin, exact-out semantics use `amountInMax` for input |
| `policy-engine-adapter-uniswap-v3/src/exact_output.rs` (unit) | 3 | round-trip, selector pin, reversed-path semantics |
| `policy-engine-adapter-uniswap-v3/src/multicall.rs` (unit) | 3 | selector pins, ABI round-trip, supported child expansion |
| `policy-engine-adapter-uniswap-v3/tests/abi_cross_check.rs` | 8 | Hand-rolled bytes vs `sol!` macro: selector, fee tiers, U256 edges, symmetric decoding |
| `policy-engine-adapter-universal-router/src/lib.rs` (unit) | 4 | execute selector pins, ABI round-trip, V3/V4 command expansion |
| `policy-engine-adapter-uniswap-v2/src/common.rs` (unit) | 4 | `shift_decimals`, `TokenLookup`, native ETH sentinel |
| `policy-engine-adapter-uniswap-v2/src/swap_*` (unit, 6 modules) | 19 | Per-function: round-trip / selector / build with native-ETH or amount-cap semantics |
| `integration-tests/tests/e2e_swap.rs` | 11 | End-to-end: USDT/USDC/WETH inputs at boundaries, stale/missing oracle, unknown target, corrupt calldata |
| `integration-tests/tests/composite_routers.rs` | 3 | Existing max-swap policy denies leaf swaps inside V3 multicall and Universal Router V3/V4 commands |
| `integration-tests/tests/policy_json.rs` | 10 | CedarJSON ↔ text parity: load via builder, mixed sources, invalid JSON, warn variant |
| `integration-tests/tests/adapter_into_request.rs` | 9 | Default `into_request`; custom `Adapter` override; hand-built `PolicyRequest`; dyn-`Adapter` collections; `Pipeline` over `&dyn AdapterRegistry`; custom `AdapterRegistry` impl |
| `integration-tests/tests/extra_swap_policies.rs` | 14 | 4 new policies (fee cap, allowlist, no-zero-min-output, USD floor): per-policy V2/V3 happy + sad paths, oracle-missing skip, and a composition test verifying deny-overrides preserves co-fired warns |
| **Total** | **143** | |

## What's deliberately not here yet

- 1inch, CowSwap, Curve, Balancer, Pendle adapters
- ERC-20 `approve` / `transfer` adapters
- Real-API oracle implementations (HTTP-backed `Oracle` impls slot into
  `crates/policy-engine/src/oracle.rs` next to `MockOracle`)
- WASM compute step for adapters that can't decode declaratively
- Manifest-driven adapters (still hardcoded in Rust)
- Marketplace, packaging, signing
- Playground UI
- Cedar Symbolic Compiler integration (publish-time + install-time
  conflict detection — see spec §8.6)

Those are the next steps, sequenced in the design doc's §11 "Implementation
roadmap (build order — playground-first)".
