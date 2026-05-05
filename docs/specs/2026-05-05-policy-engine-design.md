# Web3 Wallet Transaction Policy Engine — v1 Design

| | |
|---|---|
| Status | Draft (post-brainstorm); **v0.x reference implementation lives in this workspace** |
| Original date | 2026-05-05 |
| Scope | v1 — on-device, swap + approve, marketplace + playground |
| Out of scope | EVM simulation, on-chain enforcement, non-EVM chains, MEV protection, transaction mutation |

> **Reading guide.** This is a forward-looking design document. The companion section [Implementation status](#0a-implementation-status-snapshot) below shows exactly which parts of the spec are shipped, in progress, or deferred. Where the doc says "v1 includes X" but reality has X in a different state, the status section is authoritative; the body text preserves the original design intent.

---

## 0a. Implementation status snapshot

The reference implementation in `crates/` has progressed past the original v0.1 milestone. Net additions and divergences:

### Shipped beyond original plan

- **`Action::Multi { children }`** + per-leaf evaluation pass — a single tx can contain multiple swap leaves (Uniswap V3 `multicall(...)`, Universal Router command stream) and the per-leaf eval pass means existing swap policies apply unchanged to inner leaves. Originally this was a §5.4 sketch and a §4.5 note; now wired through `Pipeline`, `Adapter::build_actions` / `Adapter::into_requests`, `Verdict::aggregate`, and `lowering::requests_from_actions`.
- **Universal Router adapter** — decodes the Uniswap V4-aware `execute(commands, inputs[, deadline])` byte-stream into leaf swap actions, including v4 `V4_SWAP` action/params parsing and conservative context flags (`hookDataPresent`, `allowRevert`, `subPlanPresent`). Originally not in scope of §5.
- **Verdict shape** — `enum Verdict { Pass, Warn(Vec<MatchedPolicy>), Fail(Vec<MatchedPolicy>) }`, with `MatchedPolicy.severity` preserved per element so that a fail-overrides-warn aggregation does **not** drop the warn entries (warn-union invariant). The original §6.5 sketched `decision: "deny"` strings; the typed enum form is what's shipped.
- **`Pipeline` is generic** over `R: AdapterRegistry + ?Sized` and `O: Oracle` — hosts can swap registry impls (in-memory, hot-reload, remote-mirror) without changing the runtime.
- **adapters-bundle aggregator** — `default_registry()` stitches every first-party adapter into a `MockAdapterRegistry`. Adding a new adapter is a 2-line edit; no `Pipeline` recompile needed.

### Status by spec section

| Spec area | Status | Notes |
|---|---|---|
| §3 System architecture (8 components) | ✅ shipped | All eight units present in `crates/policy-engine/src/{core,oracle,policy,adapter,registry,lowering,pipeline}.rs`. |
| §4 Type system + AmountSpec A' | ✅ shipped | `core.rs`. `Action` variants: `Swap`, `Multi`, `Other`. (`Approve` deferred — see below.) |
| §4.5 Per-action + per-tx eval pass | 🟡 partial | Per-leaf pass shipped via `requests_from_actions`. Per-tx pass (`Op::"send_tx"` with `allActions` context) ⏭ — needed for cumulative policies (e.g., 24h volume). |
| §4.6 Stateful aggregation (window stats) | ⏭ deferred | No host-side stat windows yet. |
| §5 Adapter layer (manifest format) | 🟡 hand-written Rust adapters only | Declarative YAML manifest evaluator ⏭. 13 adapters shipped in Rust: V2 ×6, V3 ×5 (incl. multicall), Universal Router ×1. |
| §5.5 WASM compute escape hatch | ⏭ deferred | V3 packed-path decoded in pure Rust today; WASM sandbox not introduced. |
| §5.2 ERC-20 approve adapter | ⏳ in flight | Designed in spec, **not yet implemented** in code. The biggest remaining v0.x gap (closes the swap+approve security pair). |
| §6.1 Cedar `@severity` + default-allow | ✅ shipped | `policy.rs`. Annotation parsed at ingest, drives Pass/Warn/Fail variant. |
| §6.2 Four canonical swap policies | ✅ shipped (and one extra) | 5 policies in `policies/swap/`: `max-swap-usd-100` (the spec original), plus `max-swap-fee-bps-100`, `uniswap-only-allowlist`, `no-zero-min-output`, `min-output-usd-floor`. Slippage policy not shipped (would need adapter-side slippage derivation). |
| §6.3 Approve policies | ⏭ deferred | Designed; await §5.2 (approve adapter). |
| §6.4 Templates / form-based UX | ⏭ deferred | No template engine yet; policies hand-authored as `.cedar` / `.json`. |
| §6.5 Verdict + diagnostics | ✅ shipped | Tri-state `Verdict` enum + `MatchedPolicy { policy_id, reason, severity }`. |
| §6.6 Active policy set storage | 🟡 in-process only | `PolicyEngineBuilder::add_text` / `add_json` / `add_json_str`. No on-disk active set yet. |
| §7 Evaluation pipeline (Stage 1–4) | 🟡 most of it | Stages 1, 2, 3 (mock), 4 all present. Caching ⏭, snapshot ids on verdict ⏭, host-policy fail-closed defaults ⏭. |
| §7.5 Latency budget | n/a | Not measured yet (no perf gate in CI). |
| §7.6 Determinism guarantees | ✅ structural | Cedar pure; oracle frozen at Stage 3; no clock/random in adapters. WASM determinism profile n/a (no WASM yet). |
| §7.8 Trace format | ⏭ deferred | No trace export yet; Rust integration tests cover the same use cases for now. |
| §8 Marketplace + packaging | ⏭ deferred | No tarball/manifest format, no signing, no registry server. |
| §8.6 Defense layers | 🟡 partial | Cedar runtime (no code execution) ✅; strict resolver ✅; signing/CRL/Cedar Symbolic ⏭. |
| §9 Playground / simulator | ⏭ deferred | No browser UI. CLI replay-fixture tool also deferred — Rust integration tests substitute for now. |
| §10 Wallet integration (EOA) | ✅ contract documented | Spec §10.1 obligations are the integration contract; no reference wallet yet. |

Legend: ✅ shipped · 🟡 partial · ⏳ in flight · ⏭ deferred

### Test surface

143 tests across the workspace as of the most recent sweep:

- `policy-engine` lib unit tests: 28 (core, oracle, policy, adapter, registry, lowering, pipeline)
- `uniswap-v3` lib unit tests: 33 (across 5 swap-fn modules + multicall + common)
- `uniswap-v3` `tests/abi_cross_check.rs`: 8 (sol! macro byte-equivalence)
- `uniswap-v2` lib unit tests: 23 (across 6 swap-fn modules + common)
- `universal-router` lib unit tests: 4 (command/action stream parsing)
- `integration-tests/tests/e2e_swap.rs`: 11
- `integration-tests/tests/policy_json.rs`: 10
- `integration-tests/tests/adapter_into_request.rs`: 9
- `integration-tests/tests/extra_swap_policies.rs`: 14
- `integration-tests/tests/composite_routers.rs`: 3 (V3 multicall + UR multi-leaf paths)

### Recently amended

| Section | Change | Reason |
|---|---|---|
| §6.5 | `Verdict` is now an `enum Pass/Warn/Fail`, not a struct with a `decision` string. `MatchedPolicy` carries `severity` per element. | Compiler-enforced "host-action" semantics + warn-union when fail also fires. |
| §5.4 | Multicall expansion implemented via `Adapter::build_actions` returning `Vec<Action>` (or `Action::Multi { children }`). Selector redispatch is a Rust helper, not a manifest expansion strategy yet. | Practical for V3 multicall; sets the pattern for future declarative manifests. |
| §8.6 | Added Layer-4 "Install-time / merge-time conflict detection" — Cedar Symbolic against the *user's active policy set*, not just per-package at publish time. | A user installing policies from multiple authors can produce cross-policy conflicts even when each is internally consistent. |

---

## 0. Executive summary

We design an on-device transaction policy engine for web3 wallets. A user installs policies (either parameterized templates or hand-written Cedar policies) and the wallet consults the engine before signing each transaction. The engine returns one of `allow` / `warn` / `deny`. The engine is **wallet-agnostic** — it ships as an SDK + portable package format that any wallet can adopt.

Three core decisions shape the entire design:

1. **Policy language = Cedar (with EVM extensions) + declarative adapter manifests + narrow WASM compute escape hatch.** Cedar gives us decidability, deterministic evaluation, formal-analysis tooling, automatic diagnostics, and zero arbitrary code execution at the policy layer. Adapter manifests handle the calldata→semantic-action mapping declaratively; the rare complex decoder (e.g., Uniswap V3 packed paths) drops to a sandboxed WASM compute step.
2. **Verdict model = tri-state (`allow` / `warn` / `deny`) with deny-overrides + warn-union.** Default-allow at the engine layer ("if no policy denies, allow") matches the user's mental model that adding policies makes the wallet stricter, not looser.
3. **Determinism is design-by-construction, not enforcement.** Cedar is pure. WASM compute runs with `capability=[]` and a deterministic-float profile. External data (oracle, registry, stat windows) is fetched in a single Stage and frozen before evaluation. Every verdict carries snapshot identifiers so any decision can be reproduced byte-for-byte in the playground.

The marketplace is an open registry — anyone can publish — but trust is established by **user-controlled reviewer attestations** rather than gatekeeper curation. The playground is a fixture-replay environment that runs the *same SDK binary* as production (with mocked host capabilities), so bugs cannot be hidden by environment drift.

## 1. Goals and non-goals

### Goals

- Users can constrain their wallet's signing behavior with policies that are: secure, deterministic, fast (<250ms p99), explainable, oracle-aware, and shareable.
- Non-developers can install and parameterize templates via forms.
- Advanced users can author full Cedar policies.
- Protocol experts can author adapters that map calldata to typed semantic actions.
- All artifacts (adapter, template, bundle) are versioned, signed, and distributable through an off-chain marketplace.
- Every decision is reproducible: same input → same verdict, with a complete trace.

### Non-goals (v1)

- EVM simulation / fork-based price-impact computation.
- On-chain enforcement (e.g., ERC-4337 validator modules). Out for v1; candidate for v2.
- Transaction mutation. The engine returns verdicts, never re-writes calldata.
- MEV/private mempool policies.
- ML-based risk scoring.
- Non-EVM chains.

## 2. v1 scope summary

| Area | v1 | Deferred |
|---|---|---|
| Action kinds | `swap`, `approve`, `multi`, `other` | `transfer`, `lend`, `borrow`, `stake`, `bridge`, `claim`, `delegate`, `nft_trade` (v1.5+) |
| Evaluation location | on-device | backend, on-chain |
| Verdict | `allow` / `warn` / `deny` (deny-overrides + warn-union) | effect-based, score-based |
| Policy language | Cedar + `@severity` annotation + EVM prelude | custom DSL |
| Adapter | declarative YAML manifest + optional WASM compute | arbitrary code |
| External data | `oracle.price` (whitelisted sources), `token.metadata`, `protocol.registry`, `stat.window` | arbitrary URL fetch, simulator, custom RPC |
| Adapter resolver | strict (`chainId`, `to`, `selector`) exact match; ambiguity → user pin | wildcard fallback |
| Marketplace | open registry + user-trusted reviewer set + signed packages | curated-only, on-chain registry |
| Playground | fixture-based deterministic replay + diff + property test | EVM fork simulation |
| Reproducibility | verdict carries snapshot ids (oracle, adapter cache, policy set) | — |
| Wallet integration | EOA: SDK contract obliges host to refuse signing on `deny` verdict | 4337 validator module standard (v2) |
| Sample adapters | Uniswap V2 router, Uniswap V3 router (single + multicall), 1inch v6 (outer-only), ERC-20 `approve` | Balancer, Curve, CowSwap, Aave, Morpho, Pendle |

## 3. System architecture

### 3.1 Components

```
┌──────────────────────────── Wallet host (any wallet) ───────────────────────────┐
│                                                                                  │
│   ┌──────────────┐        ┌────────────────────┐       ┌─────────────────────┐  │
│   │ Sign request │──tx──▶ │  Policy Engine SDK │──────▶│   Result handler    │  │
│   │   (UI/RPC)   │        │  (host-embedded)   │       │ (UI: allow/warn/deny)│  │
│   └──────────────┘        └─────────┬──────────┘       └─────────────────────┘  │
│                                     │ uses                                       │
│                                     ▼                                            │
│         ┌───────────────────────────────────────────────────────────┐            │
│         │                Evaluation Pipeline                         │            │
│         │                                                            │            │
│         │  raw calldata                                              │            │
│         │      │                                                     │            │
│         │      ▼  Stage 1: Adapter Resolver (selector + addr match) │            │
│         │  decoded ABI args                                          │            │
│         │      │                                                     │            │
│         │      ▼  Stage 2: Semantic Mapper                           │            │
│         │  Action[] (typed semantic action tree, may be nested)     │            │
│         │      │                                                     │            │
│         │      ▼  Stage 3: Context Assembler                         │            │
│         │  EvalContext = { actions, wallet, oracle, env }           │            │
│         │      │                                                     │            │
│         │      ▼  Stage 4: Cedar Evaluator                           │            │
│         │  Verdict = { decision, warns[], reasons[] }               │            │
│         └───────────────────────────────────────────────────────────┘            │
│                                     │                                            │
│            ┌────────────────────────┼────────────────────────┐                   │
│            ▼                        ▼                        ▼                   │
│   Adapter Registry          Policy Set Store         Oracle Adapter              │
│   (signed bundles, cache)   (user + marketplace)    (price/registry feeds)       │
└──────────────────────────────────────────────────────────────────────────────────┘
                                     ▲
                                     │ install / update / publish
                                     │
                            ┌────────────────────┐
                            │   Marketplace      │  (off-chain registry, signed packages,
                            │   (off-chain)      │   versioning, attestations)
                            └────────────────────┘
                                     ▲
                            ┌────────────────────┐
                            │  Playground / SIM  │  (browser-based; same SDK binary +
                            │                    │   fixture loader)
                            └────────────────────┘
```

### 3.2 Responsibilities (one-line each)

| Unit | Responsibility | Input | Output | Depends on |
|---|---|---|---|---|
| Adapter Resolver | `(chainId, to, selector)` → adapter | tx metadata | adapter id or `other` or `ambiguous` | Adapter Registry |
| Semantic Mapper | apply manifest, emit `Action` tree | adapter + decoded ABI | `Action` | optional WASM compute |
| Context Assembler | pre-fetch declared deps, build Cedar entity store | `Action` + wallet state | `EvalContext` | Oracle Adapter, Token Registry |
| Cedar Evaluator | verdict from policy set | `EvalContext` + policies | `Verdict` | Cedar runtime (pure) |
| Adapter Registry | store/cache/verify adapter packages | bundle + signature | adapter loader | filesystem / IPFS / HTTP |
| Policy Set Store | active policy instances | manifest | compiled bytecode | local DB |
| Oracle Adapter | fetch + cache trusted price/registry data | `(token, chainId, …)` | numeric / metadata | Chainlink, Pyth, internal aggregator |
| Marketplace Client | search/download/verify packages | URI + version | signed package | HTTP, signature key store |
| Playground | reconstruct `EvalContext` from fixture, run SDK | tx fixture + state fixture | `Verdict` + trace | SDK |

### 3.3 Trust boundaries and core invariants

- **Cedar evaluator is a pure function** `EvalContext → Verdict`. No external calls, no time dependency. This is the foundation of determinism.
- **External data enters only at Stage 3.** Cedar code can never fetch.
- **WASM compute is restricted to adapters.** Capability set is empty — no syscalls, no network, no clock, no filesystem. Memory ≤ 4 MB, wall time ≤ 5 ms, deterministic-float profile, SIMD disabled, integrity-checked module hash.
- **Policy code (Cedar) cannot execute arbitrary code.** Cedar evaluates closed-form expressions; no eval, no recursion, no I/O. This is the structural reason why even an adversarial policy in the marketplace cannot compromise the host.

## 4. Type system and Action model

### 4.1 Primitives

EVM types are introduced as Cedar extension types in a `policy_engine` namespace:

```text
Address       :: bytes(20)              — lowercase hex; format-checked at the boundary
Bytes         :: opaque
Bytes32       :: bytes(32)
ChainId       :: long                    — EIP-155
Selector      :: bytes(4)
Uint256       :: extension `u256`        — string-encoded; ==, <, <= and +, -, *
Decimal       :: Cedar built-in          — 10^-18 precision
Timestamp     :: long (unix seconds)
```

Division and modulo on `Uint256` are deliberately not exposed inside Cedar. Wei-scale arithmetic that requires division (USD conversion, slippage) happens during Stage 3 in the host or in the manifest expression sub-language; Cedar receives the result as a `Decimal`.

### 4.2 Domain entities

```cedar-schema
namespace policy_engine {

  entity Wallet {
    address: Address,
    chainId: ChainId,
  };

  entity Token {
    chainId: ChainId,
    address: Address,            // native is the 0xeeee…eeee sentinel
    symbol: String,
    decimals: Long,
    isNative: Bool,
    listings: Set<String>,       // {"coingecko-top-100", "uniswap-default-list", "internal-allowlist"}
    deployedAt: Timestamp?,
  };

  entity Protocol {
    id: String,                  // "uniswap-v2", "uniswap-v3", "1inch-aggregation-v6", "erc20"
    kind: String,                // "dex", "token", ...
    version: String,
    chainIds: Set<ChainId>,
    audited: Bool,
    routerAddresses: Set<Address>,
  };

  entity Address_ extends Address {
    label: String?,
    tags: Set<String>,           // {"contract", "eoa", "exchange-deposit", "user-saved"}
    firstSeenAt: Timestamp?,
  };
}
```

### 4.3 AmountSpec (final form — A')

```cedar-schema
type AmountSpec = {
  token: Token,
  raw:   Uint256,                // canonical, always present
  human: Decimal?,               // raw / 10^decimals; present when token.decimals known
  usd:   UsdValuation?,          // present when adapter declared `requires: oracle.price(token)`
};

type UsdValuation = {
  value:         Decimal,
  asOfTs:        Timestamp,
  sources:       Set<String>,    // ["chainlink-eth-usd", ...]
  staleSec:      Long,           // seconds since `asOfTs` at evaluation time
  confidenceBps: Long?,          // optional dispersion across sources
};
```

Style rules (enforced by marketplace publish-time lint):

- Cross-token comparisons must use `usd.value`, not `raw` of two different tokens.
- USD-based policies must guard with `has "usd"` or pick a `failMode` (open vs closed) at template parameterization time.
- Same-token comparisons may use `raw` (exact wei) or `human` (decimals applied).

`UsdValuation` carries provenance (`sources`, `staleSec`) so policies can also gate on data quality, not just value.

### 4.4 Action shape (v1 kinds)

```cedar-schema
type Action = Swap | Approve | Multi | Other;

type Swap = {
  kind: "swap",
  protocol: Protocol,
  actor: Wallet,
  target: Address,
  valueWei: Uint256,
  inputToken: Token,
  outputToken: Token,
  inputAmount: AmountSpec,
  minOutputAmount: AmountSpec?,        // exact-in
  maxInputAmount: AmountSpec?,         // exact-out
  expectedOutputAmount: AmountSpec?,   // oracle-based approximation in v1
  recipient: Address_,
  route: List<Token>,
  deadline: Timestamp?,
  feeBips: Long?,
  derived: {
    slippageBps: Long?,
    priceImpactBps: Long?,
    effectiveFeeBps: Long?,
  },
};

type Approve = {
  kind: "approve",
  actor: Wallet,
  target: Address,                     // ERC-20 token contract
  valueWei: Uint256,
  token: Token,
  spender: Address_,
  amount: AmountSpec,
  unlimited: Bool,                     // raw close to 2^256-1
};

type Multi = {
  kind: "multi",
  actor: Wallet,
  target: Address,
  valueWei: Uint256,
  children: List<Action>,
};

type Other = {
  kind: "other",
  actor: Wallet,
  target: Address,
  selector: Selector,
  valueWei: Uint256,
  rawCalldata: Bytes,
};

action Op = ["swap", "approve", "multi", "other", "send_tx"];
```

`Other` is emitted whenever the resolver finds no matching adapter or a strict-mode failure; policies must explicitly handle it.

### 4.5 Two evaluation passes

```
Per-action pass:    for each leaf Action a:
  Cedar Request {
    principal = Wallet::"<actor>"
    action    = Op::"<a.kind>"            // "swap" | "approve" | "other"
    resource  = Protocol::"<a.protocol.id>"  (or Protocol::"unknown")
    context   = action_to_record(a)
  }

Per-tx pass:        once per transaction:
  Cedar Request {
    principal = Wallet::"<from>"
    action    = Op::"send_tx"
    resource  = Address_::"<tx.to>"
    context   = { tx, allActions, windowStats, ... }
  }
```

Verdict aggregation:

- Any matched `forbid` with `@severity("deny")` → `Verdict.deny`.
- Otherwise, any matched `forbid` with `@severity("warn")` → `Verdict.warn` (messages unioned).
- Otherwise → `Verdict.allow`.

### 4.6 Stateful aggregation

Cedar is stateless. "Cumulative volume in last 24h" lives in `context.windowStats`, populated by the host before Stage 4:

```text
context.windowStats = {
  swap_volume_usd_24h: Decimal,
  swap_volume_usd_7d:  Decimal,
  unique_recipients_30d: Long,
  ...
}
```

Templates declare `requires: stat.window.<key>` so the host knows which windows to maintain. The maintenance is host-side bookkeeping; Cedar still sees a deterministic snapshot.

## 5. Adapter layer

### 5.1 Manifest format

> **Status (sweep): ⏭ deferred.** Adapters today are hand-written Rust crates (one per protocol/function). The declarative YAML manifest format described below is the v0.5+ target; it's what makes adapter authoring marketplace-installable without a Rust recompile.

```yaml
schemaVersion: "policy-engine/adapter/1"

id: "uniswap-v2/swapExactTokensForTokens"
protocol: "uniswap-v2"
version: "1.0.0"
author: "did:key:z6Mk..."
description: "Uniswap V2 Router: exact-in token-to-token swap"

match:
  chainIds: [1, 10, 137, 8453, 42161]
  targets:
    1:    "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
    10:   "0x..."
    137:  "0x..."
    8453: "0x..."
    42161: "0x..."
  selector: "0x38ed1739"
  abi: |
    swapExactTokensForTokens(
      uint256 amountIn,
      uint256 amountOutMin,
      address[] path,
      address recipient,
      uint256 deadline
    ) returns (uint256[])

decode:
  args: abi

emit:
  kind: swap
  protocol: { ref: "uniswap-v2" }
  fields:
    actor:             { ref: "$tx.from" }
    target:            { ref: "$tx.to" }
    valueWei:          { ref: "$tx.value" }
    inputToken:        { tokenLookup: "$path[0]"  on: "$tx.chainId" }
    outputToken:       { tokenLookup: "$path[-1]" on: "$tx.chainId" }
    route:             { tokenLookupList: "$path"  on: "$tx.chainId" }
    inputAmount:
      token:  { ref: "$path[0]" }
      raw:    { ref: "$amountIn" }
      human:  { compute: "toHuman($amountIn, inputToken.decimals)" }
      usd:
        compute:  "toUsdValuation($amountIn, oracle.price(inputToken))"
        requires: ["oracle.price(inputToken)"]
        optional: true
    minOutputAmount:
      token:  { ref: "$path[-1]" }
      raw:    { ref: "$amountOutMin" }
      human:  { compute: "toHuman($amountOutMin, outputToken.decimals)" }
      usd:
        compute:  "toUsdValuation($amountOutMin, oracle.price(outputToken))"
        requires: ["oracle.price(outputToken)"]
        optional: true
    expectedOutputAmount:
      compute:  "spotConvert($amountIn, oracle.price(inputToken), oracle.price(outputToken))"
      requires: ["oracle.price(inputToken)", "oracle.price(outputToken)"]
      optional: true
    recipient:         { addrLookup: "$recipient" }
    deadline:          { ref: "$deadline" }
  derived:
    slippageBps:
      compute:  "((expectedOutputAmount.raw - $amountOutMin) * 10000) / expectedOutputAmount.raw"
      requires: ["expectedOutputAmount"]
      optional: true

tests:
  - name: "USDC→WETH on mainnet"
    input:
      chainId: 1
      to:      "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
      from:    "0xUser"
      value:   "0"
      data:    "0x38ed1739..."
    fixtures:
      tokens:
        "0xA0b8...USDC": { decimals: 6, symbol: "USDC" }
        "0xC02a...WETH": { decimals: 18, symbol: "WETH" }
      oracle:
        "USDC": { usd: "1.00" }
        "WETH": { usd: "3000.00" }
    expect:
      kind: swap
      protocol.id: "uniswap-v2"
      inputToken.symbol: "USDC"
      outputToken.symbol: "WETH"
      inputAmount.raw: "1000000000"
      inputAmount.usd.value: "1000.00"
```

The manifest expression sub-language is intentionally narrow: `$bind` references, indexing/slicing (`$path[0]`, `$path[-1]`, `$path[1:-1]`), `len()`, arithmetic (+, -, *), comparisons, and host-provided capabilities (`tokenLookup`, `addrLookup`, `oracle.price`, `host.<expansion>`). No user-defined functions.

### 5.2 ERC-20 `approve` adapter (new in v1)

> **Status (sweep): ⏳ designed but not implemented.** The 13 adapters shipped today are all swap/composite (V2 ×6, V3 ×5, Universal Router). Approve is the highest-value remaining v0.x gap because swap policies don't catch the "unlimited approval" attack surface on their own.

```yaml
schemaVersion: "policy-engine/adapter/1"

id: "erc20/approve"
protocol: "erc20"
version: "1.0.0"
description: "Standard ERC-20 approve(spender, amount)"

match:
  chainIds: [1, 10, 137, 8453, 42161, 56, 43114]
  targets: ["*"]                         # any contract that implements ERC-20
  selector: "0x095ea7b3"
  abi: |
    approve(address spender, uint256 amount) returns (bool)

decode:
  args: abi

emit:
  kind: approve
  fields:
    actor:    { ref: "$tx.from" }
    target:   { ref: "$tx.to" }
    valueWei: { ref: "$tx.value" }
    token:    { tokenLookup: "$tx.to" on: "$tx.chainId" }
    spender:  { addrLookup: "$spender" }
    amount:
      token:  { ref: "$tx.to" }
      raw:    { ref: "$amount" }
      human:  { compute: "toHuman($amount, token.decimals)" }
      usd:
        compute:  "toUsdValuation($amount, oracle.price(token))"
        requires: ["oracle.price(token)"]
        optional: true
    unlimited:
      compute: "$amount >= 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffeffffffff"
      # i.e., >= 2^256 - 2^32; covers MaxUint256 minus small slop

tests:
  - name: "USDC unlimited approve to Uniswap router"
    input:
      chainId: 1
      to:      "0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48"   # USDC
      from:    "0xUser"
      data:    "0x095ea7b3
                0000000000000000000000007a250d5630B4cF539739dF2C5dAcb4c659F2488D
                ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    expect:
      kind: approve
      spender: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
      unlimited: true
```

`targets: ["*"]` indicates any contract address — there is no single canonical ERC-20 deployment. The Adapter Resolver still requires a strict (chainId, selector) match; the wildcard target is allowed only because `approve` is a standardized selector that token contracts implement universally. Markets MAY pin specific token addresses for stricter behavior.

### 5.3 Adapter Resolver — strict exact-match

```
input: tx = { chainId, to, value, data }

1. selector = data[0:4]
2. key = (chainId, to, selector)
3. matches = adapterCache.get(key)               // host-side cache
   if cache miss:
     matches = registry.lookup(key)              // network or local index
     for each m where m.match.targets[chainId] == "*":
       matches += m
     adapterCache.put(key, matches, ttl=24h)
4. signature-verify each match against trusted-signer set
5. switch len(verified_matches):
     0  → emit Action.kind = "other"
     1  → return matches[0]
    >1  → if user has pinned a choice for this key:
            return pinned
          else:
            return AmbiguousResolution(matches)   // host UI prompts user once, persists pin
6. (Optionally) the host may run in *strict mode* where ambiguity always denies until pinned.
```

The registry is queried via host-side cache; transaction-time evaluation never makes a network call. Cache invalidation is on user trigger or background refresh. Verdicts include the cache snapshot id so reproductions can rebuild state.

### 5.4 Multicall handling

> **Status (sweep): ✅ shipped.** Implemented for Uniswap V3 `multicall(bytes[])` / `multicall(uint256,bytes[])` and for the Universal Router `execute(...)` command stream. Selector redispatch is currently a Rust helper rather than a manifest expansion strategy; the declarative manifest form remains future work.

Routers like Uniswap V3 expose `multicall(deadline, bytes[] data)`. The adapter emits `kind: multi` and recursively expands children:

```yaml
id: "uniswap-v3/multicall"
match:
  chainIds: [1, 10, 137, 8453, 42161]
  targets: { 1: "0x68b3...SwapRouter02", ... }
  selector: "0x5ae401dc"
decode:
  args: abi
emit:
  kind: multi
  fields:
    actor:    { ref: "$tx.from" }
    target:   { ref: "$tx.to" }
    valueWei: { ref: "$tx.value" }
  children:
    expand: "$data"
    via:    "selectorRedispatch"      # host built-in: each $data[i] becomes a virtual sub-tx
                                       # (chainId=$tx.chainId, to=$tx.to, data=$data[i])
                                       # re-entered into the resolver, recursively
```

### 5.5 Uniswap V3 packed-path decoding via WASM compute

> **Status (sweep): ⏭ deferred.** V3 `exactInput` / `exactOutput` packed-path decoding is implemented in pure Rust today (`uniswap_v3::common::decode_v3_path`). The WASM compute escape hatch becomes useful only when (a) the manifest format ships and (b) we encounter a calldata shape that isn't expressible declaratively. Until then, escape-hatch adapters are just Rust.

`exactInput((bytes path, address recipient, uint256 amountIn, uint256 amountOutMinimum))` cannot be decoded purely by manifest because `path` is a packed bytes blob. The adapter declares a WASM compute step:

```yaml
id: "uniswap-v3/exactInput"
match: { ... selector: "0xb858183f" ... }
decode:
  args: abi
emit:
  kind: swap
  compute:
    module:     "uniswap-v3-path-decode.wasm"
    sha256:     "abc123..."
    entrypoint: "decodeExactInput"
    input:      { params: "$params" }
    output:
      route:           "List<Token>"
      inputToken:      "Token"
      outputToken:     "Token"
      feeBipsAvg:      "Long"
  fields:
    inputAmount:
      raw: { ref: "$params.amountIn" }
      ...
    minOutputAmount:
      raw: { ref: "$params.amountOutMinimum" }
      ...
    # route, inputToken, outputToken filled from compute output
```

WASM execution constraints (host-enforced, non-negotiable):

- Memory ≤ 4 MB.
- Wall time ≤ 5 ms.
- `capabilities = []` (no WASI, no syscalls, no clock, no random, no network, no filesystem).
- Deterministic-float profile, SIMD disabled.
- Module hash `sha256` is part of the manifest; mismatched binary fails to load.
- Allowed exports: only the declared entrypoint with a typed schema.

Failure modes (timeout, trap, syscall attempt, bad output type) cause the surrounding evaluation to **deny** — a misbehaving adapter never silently produces a valid Action.

### 5.6 1inch / aggregator — outer-only

Aggregator routers loop through internal DEXes. Inner-call enumeration requires simulation, which is out of v1 scope. We emit only the outer-level user intent:

```yaml
id: "1inch/aggregation-v6/swap"
analysisDepth: "outer-only"
match:
  selector: "0x07ed2379"
  targets: { 1: "0x111111125421cA6dc452d289314280a0f8842A65" }
emit:
  kind: swap
  fields:
    actor:               { ref: "$tx.from" }
    inputToken:          { tokenLookup: "$desc.srcToken" }
    outputToken:         { tokenLookup: "$desc.dstToken" }
    inputAmount.raw:     { ref: "$desc.amount" }
    minOutputAmount.raw: { ref: "$desc.minReturnAmount" }
    recipient:           { addrLookup: "$desc.dstReceiver" }
```

The `analysisDepth: "outer-only"` flag is metadata that templates can read; templates that depend on inner-call accuracy will refuse to install for outer-only adapters or will warn the user.

## 6. Policy layer

### 6.1 Cedar with `@severity` annotation

```cedar
@id("...")
@severity("deny" | "warn")
@reason("human-readable explanation")
forbid (
  principal,
  action == Op::"<kind>",
  resource is <EntityType>
) when    { boolean expression };
unless    { boolean expression };
```

The host evaluator wraps Cedar:

- Cedar's native model is default-deny + permit; we configure default-allow + forbid (forbid-only policies). Verdict aggregation looks at matched `forbid` clauses and reads `@severity` to decide deny vs warn.
- `permit` clauses are not used by user policies in v1. (Reserved for future use.)
- `@reason` is the policy author's explanation, optionally i18n-keyed; Cedar's automatic diagnostics provide the matched clause and bindings.

### 6.2 The four canonical swap policies

#### 6.2.1 Swap fee cap

```cedar
@id("user/max-swap-fee-bps")
@severity("deny")
@reason("Swap fee exceeds the configured cap")
forbid (principal, action == Op::"swap", resource)
when {
  context.feeBips != null &&
  context.feeBips > 100        // 100 bps = 1.0%
};
```

Companion policy when adapters lack fee data:

```cedar
@id("user/require-fee-disclosure")
@severity("warn")
@reason("This adapter does not disclose fee information")
forbid (principal, action == Op::"swap", resource)
when { context.feeBips == null };
```

#### 6.2.2 Maximum single-swap USD

```cedar
@id("user/max-single-swap-usd")
@severity("deny")
@reason("Single swap USD amount exceeds the cap (5000)")
forbid (principal, action == Op::"swap", resource)
when {
  context.inputAmount has "usd" &&
  context.inputAmount.usd.staleSec <= 60 &&
  context.inputAmount.usd.value > decimal("5000.00")
};
```

Strict (fail-closed) variant — denies when oracle data is missing or stale:

```cedar
@id("user/max-single-swap-usd-strict")
@severity("deny")
@reason("Cannot determine USD value — denying for safety")
forbid (principal, action == Op::"swap", resource)
when {
  !(context.inputAmount has "usd") ||
  context.inputAmount.usd.staleSec > 60 ||
  context.inputAmount.usd.value > decimal("5000.00")
};
```

#### 6.2.3 Slippage tolerance cap

```cedar
@id("user/max-slippage-bps")
@severity("deny")
@reason("Slippage tolerance exceeds 1.0%")
forbid (principal, action == Op::"swap", resource)
when {
  context.derived has "slippageBps" &&
  context.derived.slippageBps > 100
};
```

In v1, `slippageBps` is computed from oracle spot prices, not on-chain quotes. Templates that use it should disclose this limitation (`expectedOutputAmount` in v1 is a spot-price approximation, not an AMM quote).

#### 6.2.4 "If using Uniswap, only via Router"

Convention: adapters that hit a router emit `protocol.id = "uniswap-v2"`; hypothetical adapters that decode direct-pair calls emit `protocol.id = "uniswap-v2-pair"` or `"uniswap-v2-direct"`.

```cedar
@id("user/uniswap-router-only")
@severity("deny")
@reason("Uniswap swaps must go through the official Router")
forbid (principal, action == Op::"swap", resource is Protocol)
when {
  resource.id like "uniswap-*-pair" ||
  resource.id like "uniswap-*-direct"
};
```

Or by router-address whitelist:

```cedar
@id("user/uniswap-router-only-by-addr")
@severity("deny")
@reason("Call did not target a known Uniswap router")
forbid (principal, action == Op::"swap", resource is Protocol)
when {
  resource.id like "uniswap-*" &&
  !(resource.routerAddresses.contains(context.target))
};
```

### 6.3 Approve policies (new in v1)

> **Status (sweep): ⏭ deferred.** Awaits §5.2 (ERC-20 approve adapter). Sketches below are the design target.

#### 6.3.1 Block unlimited approval

```cedar
@id("user/no-unlimited-approval")
@severity("deny")
@reason("Unlimited (max-uint) approvals are blocked")
forbid (principal, action == Op::"approve", resource)
when { context.unlimited };
```

#### 6.3.2 Approve spender allowlist

```cedar
@id("user/approve-known-spenders-only")
@severity("warn")
@reason("Approving an unknown spender")
forbid (principal, action == Op::"approve", resource)
when {
  !(context.spender.tags.contains("user-saved")) &&
  !(context.spender.tags.contains("known-router"))
};
```

#### 6.3.3 Maximum approve USD

```cedar
@id("user/max-approve-usd")
@severity("deny")
@reason("Approval USD value exceeds the cap")
forbid (principal, action == Op::"approve", resource)
when {
  context.amount has "usd" &&
  context.amount.usd.staleSec <= 60 &&
  context.amount.usd.value > decimal("10000.00")
};
```

### 6.4 Templates and the form-based UX

> **Status (sweep): ⏭ deferred.** Policies today are hand-authored as `.cedar` text or CedarJSON; both forms load via `PolicyEngineBuilder::add_text` / `add_json`. The parameterized template + form-renderer described below is what makes policies non-developer-installable. Pending until marketplace work begins.

```yaml
schemaVersion: "policy-engine/template/1"
id: "core/max-single-swap-usd"
version: "1.0.0"
title: "Maximum single-swap USD"
shortDescription: "Reject swaps with input USD value above a configured cap."

severity: "deny"

params:
  - name: maxUsd
    type: decimal
    label: "Maximum USD amount"
    default: "1000.00"
    min: "1.00"
    max: "10000000.00"
  - name: failMode
    type: enum
    label: "Behavior when oracle is unavailable"
    values:
      - { value: "open",   label: "Allow (fail-open)" }
      - { value: "closed", label: "Deny (fail-closed)" }
    default: "open"
  - name: maxStaleSec
    type: long
    label: "Maximum oracle staleness (seconds)"
    default: 60
    min: 5
    max: 600

# `template` is a host-side string template (Handlebars-style); it is rendered to
# Cedar source by literal substitution of `params`, then parsed and validated.
template: |
  @id("{{policyId}}")
  @severity("deny")
  @reason("Single swap USD amount exceeds {{maxUsd}}")
  forbid (principal, action == Op::"swap", resource)
  when {
    {{#if failMode == "closed"}}
    !(context.inputAmount has "usd") ||
    context.inputAmount.usd.staleSec > {{maxStaleSec}} ||
    {{/if}}
    (context.inputAmount has "usd" &&
     context.inputAmount.usd.staleSec <= {{maxStaleSec}} &&
     context.inputAmount.usd.value > decimal("{{maxUsd}}"))
  };

tests:
  - name: "1500 USD swap exceeds 1000 limit → deny"
    params: { maxUsd: "1000.00", failMode: "open", maxStaleSec: 60 }
    fixture: "fixtures/usdc-to-weth-1500usd.yaml"
    expect: { decision: "deny" }
  - name: "500 USD swap below limit → allow"
    params: { maxUsd: "1000.00", failMode: "open", maxStaleSec: 60 }
    fixture: "fixtures/usdc-to-weth-500usd.yaml"
    expect: { decision: "allow" }
  - name: "no oracle, fail-open → allow"
    params: { maxUsd: "1000.00", failMode: "open", maxStaleSec: 60 }
    fixture: "fixtures/usdc-to-weth-no-oracle.yaml"
    expect: { decision: "allow" }
  - name: "no oracle, fail-closed → deny"
    params: { maxUsd: "1000.00", failMode: "closed", maxStaleSec: 60 }
    fixture: "fixtures/usdc-to-weth-no-oracle.yaml"
    expect: { decision: "deny" }
```

The host UI auto-renders a form from `params`, instantiates the template by literal substitution, and adds the resulting Cedar policy to the user's active set. The user never reads Cedar.

### 6.5 Verdict and diagnostics

```json
{
  "decision": "deny",
  "matchedPolicies": [
    {
      "policyId": "user/max-single-swap-usd",
      "severity": "deny",
      "annotations": {
        "reason": "Single swap USD amount exceeds the cap (5000)"
      },
      "diagnostics": {
        "matchedClause": "context.inputAmount.usd.value > decimal(\"5000.00\")",
        "bindings": {
          "context.inputAmount.usd.value":   "7234.50",
          "context.inputAmount.usd.staleSec": "12",
          "context.inputAmount.usd.sources":  ["chainlink-eth-usd"]
        }
      }
    }
  ],
  "warnings": [],
  "snapshots": {
    "policySetVersion":      "ps-v17",
    "adapterCacheSnapshotId": "ac-3a91",
    "oracleSnapshotIds":     ["chainlink-eth-usd@block#19234567"]
  },
  "evaluatedAt": "2026-05-05T12:34:56Z",
  "elapsedMs": 34.4
}
```

### 6.6 Active policy set storage

```json
{
  "active": [
    {
      "instanceId": "p1",
      "templateId": "core/max-single-swap-usd",
      "templateVersion": "1.0.0",
      "params": { "maxUsd": "5000.00", "failMode": "open", "maxStaleSec": 60 },
      "compiledCedar": "@id(\"user/max-single-swap-usd\")\n@severity(\"deny\")\n...",
      "enabled": true,
      "addedAt": "2026-05-04T10:00:00Z"
    }
  ]
}
```

At evaluation time, all `enabled` instances' compiled Cedar are concatenated into a single Cedar `PolicySet` and evaluated.

## 7. Evaluation pipeline (detail)

### 7.1 Stage 1 — Adapter Resolver

See §5.3. Inputs: tx metadata. Outputs: `ResolvedAdapter | "other" | AmbiguousResolution`. The resolver never performs a network call on the critical path; the registry is host-cached.

### 7.2 Stage 2 — Semantic Mapper

```
1. ABI decode using adapter.match.abi
2. Optional WASM compute step (if adapter has `compute:`):
     wasmHost.run(module, sha256, entrypoint, input,
                  memLimit=4MB, timeLimit=5ms, capabilities=[])
3. Multicall expansion (if kind == "multi"):
     for each child_call in expand_strategy(adapter, args):
       child_action = recurse(stage1, stage2)(virtual_sub_tx)
       children.append(child_action)
4. Construct Action from manifest `emit` spec, leaving oracle-dependent fields as PLACEHOLDER for Stage 3.
```

Total Stage 2 time budget: ≤ 10 ms typical, ≤ 15 ms p99.

### 7.3 Stage 3 — Context Assembler

```
1. Collect deps from action tree (oracle.price(...), token.metadata(...), stat.window.<key>, ...).
2. Pre-fetch all deps in parallel; check freshness; verify signatures/attestations where available.
3. Evaluate each manifest `compute` expression with deps resolved; populate AmountSpec.usd, etc.
4. Build Cedar entity store and per-action context records.
5. If a required (non-optional) dep is missing → ABORT with "data unavailable" → host fails closed.
```

Whitelisted oracle sources only. Staleness gate (default 60 s, configurable per policy). Total time: typically 20–50 ms (cache-warm), p99 ~ 200 ms.

### 7.4 Stage 4 — Cedar Evaluator (pure)

```
1. Per-action pass: for each leaf action, build a Cedar Request and call cedar.is_authorized().
2. Per-tx pass: build the send_tx Request and call cedar.is_authorized().
3. Aggregate verdicts using deny-overrides + warn-union.
```

Cedar evaluation is pure. Time budget: ≤ 5 ms for ≤ 100 active policies.

### 7.5 Latency budget

| Stage | Typical | p99 |
|---|---|---|
| 1 Resolver | < 1 ms (cache hit) | 5 ms |
| 2 Mapper | 1–3 ms (no WASM), 5–10 ms (with WASM) | 15 ms |
| 3 Context Assembler | 20–50 ms | 200 ms |
| 4 Cedar | < 1 ms (small set), 5 ms (large) | 10 ms |
| **Total** | **30–60 ms** | **~250 ms** |

Stage 3 is the critical path. Hardware-wallet UX may use a `cache-only` mode that skips oracle refresh.

### 7.6 Determinism guarantees

| Potential source of nondeterminism | Mitigation |
|---|---|
| WASM clock/random | empty capability set; calls trap |
| WASM float drift | Wasmtime deterministic-float profile |
| WASM SIMD drift | SIMD feature disabled |
| Cedar external calls | Cedar has no such mechanism |
| Oracle-time drift | Stage 3 freezes; oracle snapshot ids attached to verdict |
| Registry race | adapter cache snapshot id attached to verdict |
| Policy evaluation order | Cedar evaluates all matched forbid clauses; order-independent |

Every verdict carries `oracleSnapshotIds`, `adapterCacheSnapshotId`, `policySetVersion`. These are sufficient to reproduce the verdict byte-for-byte in the playground.

### 7.7 Error handling

| Stage | Failure | Default behavior |
|---|---|---|
| 1 | no match | emit `kind: other` |
| 1 | ≥ 2 matches, no pin | UI prompt; deny until pinned |
| 1 | bad signature | emit `kind: other` |
| 2 | ABI decode fail | emit `kind: other` |
| 2 | WASM trap or timeout | **deny** |
| 3 | required dep missing | **deny** (fail-closed by default) |
| 3 | oracle staleness exceeded | **deny** |
| 4 | Cedar parse error in one policy | disable that policy, notify user; other policies still run |
| 4 | Cedar runtime error | treat as forbid (fail-closed) |

### 7.8 Trace format

```json
{
  "traceId": "...",
  "txInput": { ... },
  "stage1": { "adapter": "uniswap-v2/swapExactTokensForTokens@1.0.0", "elapsedMs": 0.4 },
  "stage2": { "decoded": {...}, "computeStepUsed": false, "elapsedMs": 1.2 },
  "stage3": {
    "deps": [
      { "id": "oracle.price(USDC)", "value": "1.00", "source": "chainlink", "ageMs": 12000 },
      { "id": "oracle.price(WETH)", "value": "3000.00", "source": "chainlink", "ageMs": 9500 }
    ],
    "elapsedMs": 32.0
  },
  "stage4": {
    "matchedPolicies": [...],
    "verdict": "deny",
    "elapsedMs": 0.8
  },
  "totalElapsedMs": 34.4
}
```

Trace format is identical between production and playground, enabling export/import-based debugging.

## 8. Marketplace and packaging

> **Status (sweep): ⏭ deferred in full.** No packaging, signing, registry server, or trust slider yet. The shape below is the v0.5 target.

### 8.1 Package types

| `kind` | Contents |
|---|---|
| `adapter` | calldata→action mapping; one per (protocol, function) |
| `template` | parameterized policy |
| `bundle` | recommended preset of templates |
| `fixture` | reusable test fixture set (publishable; v1 supports installing fixture packages from marketplace, used by playground/property-tests) |

### 8.2 Package layout (tarball)

```
my-package-1.0.0.tar.gz
├── manifest.yaml
├── README.md
├── CHANGELOG.md
├── LICENSE
├── adapter.yaml   | template.yaml   | bundle.yaml   | fixtures/...
├── compute/
│   ├── decode.wasm
│   └── decode.wasm.sha256
├── policies/                   (when template materializes raw cedar)
│   └── *.cedar
├── tests/
│   ├── fixtures/
│   └── cases.yaml
├── i18n/
│   ├── ko.json
│   └── en.json
└── SIGNATURE
```

### 8.3 Manifest fields (full)

```yaml
schemaVersion: "policy-engine/package/1"

kind: "template"
id: "kr.upside/max-single-swap-usd"
version: "1.0.0"
displayName: { en: "Max single swap USD", ko: "단일 swap USD 한도" }
shortDescription: { en: "...", ko: "..." }

author:
  did:    "did:key:z6Mkz..."
  name:   "Upside Security"
  url:    "https://upside.example/security"
  contact: "security@upside.example"

runtime:
  engineVersion: ">=1.0.0 <2.0.0"
  cedarSchemaVersion: "policy-engine/cedar/1"
  adapterSchemaVersion: "policy-engine/adapter/1"

supportedChains: [1, 10, 137, 8453, 42161]
supportedProtocols: ["uniswap-v2", "uniswap-v3", "1inch-aggregation-v6"]
supportedActions: ["swap"]

requires:
  - "oracle.price"

riskLevel: "low"        # info | low | medium | high
auditReviews:
  - reviewer: "did:key:z6Mk...auditor1"
    date: "2026-04-12"
    verdict: "passed"
    reportUrl: "ipfs://Qm..."
    scope: "manifest+template+tests"
    signatureUrl: "ipfs://Qm.../sig.json"

dependencies:
  - id:    "core/swap-context-shared"
    range: "^1.0.0"
    integrity: "sha256-..."

permissions:
  read:
    - "context.action.swap.*"
    - "context.windowStats.swap_volume_usd_24h"
    - "context.oracle.price.*"
  write: []

tests:
  testFile: "tests/cases.yaml"
  minCoverage: 0.8
  required: true

tags: ["swap", "limit", "usd", "beginner-friendly"]
category: "spending-limits"
homepage: "https://upside.example/templates/max-single-swap-usd"
issues: "https://github.com/upside/policies/issues"
licenseSpdx: "MIT"

changelog:
  - version: "1.0.0"
    date: "2026-05-01"
    changes: ["Initial release"]
```

### 8.4 Signing

```
SIGNATURE = {
  contentHash: "sha256-<tarball except SIGNATURE>",
  authorSignature: { did, sig },
  reviewerAttestations: [ { did, issuedAt, verdict, contentHashCovered, sig }, ... ],
  registryEndorsement: { registryId, issuedAt, sig }    // optional
}
```

The host SDK verifies `contentHash`, `authorSignature`, ≥ N attestations from user-trusted reviewers (N is a user setting), and the optional registry endorsement.

### 8.5 Trust model — user-controlled, not gatekept

- Open registry: anyone can publish.
- The user maintains a list of trusted reviewer DIDs.
- UI classifies each package as `Trusted` (≥ 2 trusted-reviewer attestations), `Author-signed` (signed but no review), or `Unverified` (no signature or invalid).
- Trusted: 1-click install. Author-signed: confirmation required. Unverified: blocked by default; explicit override needed.

### 8.6 Defense layers against malicious packages

1. **Structural**: Cedar cannot execute code; WASM compute has empty capability set. Worst-case adapter misbehavior is incorrect Action emission, never host compromise.
2. **Resolver**: per-(chainId, to, selector) key has at most one pinned adapter; new adapters require explicit user action.
3. **Publish-time linting** (per-package, runs in the marketplace publish gate):
   - Cedar parse + type-check + schema match
   - Cedar Symbolic Compiler self-conflict detection — the open-source Cedar tool that translates policies into SMT constraints to detect cases where the same input matches both a `permit` and a `forbid`, or where invariants the author asserts cannot hold
   - WASM static analysis (forbidden imports, float instructions, deterministic profile)
   - declared `permissions` ⊇ actual usage surface
   - declared `requires` ⊇ data dependencies in manifest expressions
   - Cross-token raw-comparison lint
4. **Install-time / merge-time conflict detection** (per-user, runs on the host SDK whenever the *active* policy set changes):
   - Each individually-published package is self-consistent (Layer 3), but **users typically install policies from multiple authors** — those policies can conflict even when each is internally clean.
   - The host SDK runs Cedar Symbolic against the *current installed policy set* on (i) every `install`, (ii) every `update`, (iii) optionally on a scheduled background sweep.
   - Detected issues:
     - **Cross-policy conflict** — two policies disagree on the same input (e.g., one always allows, another always denies the same swap).
     - **Redundancy** — policy A strictly implies policy B; B is a no-op or A is.
     - **Vacuous policy** — an installed policy that never matches anything (often a sign of a misconfigured parameter).
     - **Severity-mix** — for templates with both `warn` and `deny` variants, detect when warn fires *exactly* iff deny fires (the warn is dead).
   - Counterexample inputs from Cedar Symbolic are surfaced to the user with the conflicting policy ids and a representative `EvalContext`, so the user can decide which policy to keep, which to disable, and which to file as a publisher bug.
   - Failure mode: the engine runs with the active set anyway (the engine still evaluates correctly for any concrete input — conflicts are about *coverage gaps* and *redundancy*, not runtime safety). The conflict report is advisory, surfaced via UI, not a hard block.
5. **Dependency isolation**: strict SemVer + integrity hashes + lockfile.
6. **Incident response**: revocation list (CRL); host SDK consults CRL on install/update.

### 8.7 Versioning and dependencies

- SemVer strict for adapter, template, bundle, fixture.
- MAJOR for breaking schema changes; MINOR for additive fields; PATCH for fixes.
- Dependency resolution + integrity-hashed lockfile happen on the host at install time.
- `runtime.engineVersion` gates SDK compatibility.

### 8.8 Marketplace API

```
GET  /packages?q=&kind=&chain=&protocol=
GET  /packages/{id}/versions
GET  /packages/{id}/{version}                  → tarball
GET  /packages/{id}/{version}/manifest         → manifest.yaml (fast preview)
GET  /reviewers/{did}/attestations
GET  /crl
POST /packages                                 → publish
POST /attestations                             → reviewer attestation
```

Server is a content-addressable store with signature verification and federation-friendly mirroring; it does not exercise editorial control beyond schema validation.

## 9. Playground and simulator

> **Status (sweep): ⏭ deferred.** Rust integration tests in `crates/integration-tests/tests/` cover the same use cases the playground was meant to address (deterministic fixture replay, diff between policy versions, per-fixture verdicts). A `replay-fixture` CLI is the natural next step before any browser UI.

The playground is fixture-replay over the SDK, not EVM simulation. EVM simulation is out of v1 scope.

### 9.1 Architecture

```
   ┌───────────────── Browser (or CLI) ──────────────┐
   │                                                  │
   │   Playground UI                                  │
   │   ┌─────────────────────────────────────────┐    │
   │   │ Fixture editor │ Policy editor │ Diff   │    │
   │   └─────────────────────────────────────────┘    │
   │                  │                                │
   │                  ▼                                │
   │   Same Policy Engine SDK (WASM/JS build)         │
   │                  │                                │
   │                  ▼                                │
   │   Verdict + detailed Trace                       │
   └──────────────────────────────────────────────────┘
```

The playground swaps host capabilities (oracle.fetch, registry.lookup, stat.window) for fixture-backed mocks. Same SDK binary in both environments; the same input deterministically reproduces.

### 9.2 Fixture format

```yaml
schemaVersion: "policy-engine/fixture/1"

tx:
  chainId: 1
  from:    "0xUser..."
  to:      "0x7a25..."
  value:   "0"
  gas:     "200000"
  data:    "0x38ed1739..."
  nonce:   42

wallet:
  address: "0xUser..."
  chainId: 1
  balances:
    "0xA0b8...USDC": "5000000000"
    "0xC02a...WETH": "1000000000000000000"
  addressBook:
    "0xRecipient...": { label: "My EOA", tags: ["user-saved"] }

oracle:
  prices:
    "1:0xA0b8...USDC": { usd: "1.00",     source: "chainlink", ageMs: 12000 }
    "1:0xC02a...WETH": { usd: "3000.00",  source: "chainlink", ageMs: 9500 }

tokenRegistry:
  "1:0xA0b8...USDC": { symbol: "USDC", decimals: 6, listings: ["uniswap-default"] }
  "1:0xC02a...WETH": { symbol: "WETH", decimals: 18, listings: ["uniswap-default"] }

protocolRegistry:
  "uniswap-v2":
    kind: "dex"
    routerAddresses: ["1:0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"]

stats:
  swap_volume_usd_24h: "350.00"
  swap_volume_usd_7d:  "2400.00"

adapterPins:
  "1:0x7a25...:0x38ed1739": "uniswap-v2/swapExactTokensForTokens@1.0.0"

engineConfig:
  failClosedDefault: true
  oracleMaxStalenessSec: 60
```

Fixtures are produced by hand or exported from production traces:

```
$ policy-engine trace export <traceId> > tests/fixtures/from-prod.yaml
```

### 9.3 Test cases

```yaml
schemaVersion: "policy-engine/testcase/1"

policySetUnderTest:
  - templateInstance:
      templateId: "kr.upside/max-single-swap-usd"
      version: "1.0.0"
      params: { maxUsd: "1000.00", failMode: "open", maxStaleSec: 60 }

cases:
  - name: "1500 USD swap exceeds 1000 limit → deny"
    fixture: "fixtures/usdc-to-weth-1500usd.yaml"
    expect:
      decision: "deny"
      matchedPolicies: ["user/max-single-swap-usd"]
      reasonContains: "USD"

  - name: "property: under-limit never denied"
    type: "property"
    fixtureGenerator:
      base: "fixtures/usdc-to-weth-base.yaml"
      mutate:
        - field: "tx.data#$amountIn"
          range: { type: "uint256", min: "0", max: "999_999_900" }
    iterations: 100
    expect: { decisionNotIn: ["deny"] }
```

### 9.4 UI panes

1. **Fixture editor** — YAML or form-driven; calldata decoder, token resolver, oracle pre-filler are convenience helpers.
2. **Policy editor** — Cedar with live syntax highlighting and lint.
3. **Verdict + trace** — decision, matched policies, bindings, per-stage timing.
4. **Diff** — compare two versions of a policy on the same fixture set; classify each case as unchanged / changed / broken.

The diff pane is what publishers use to understand the impact of changes before publishing.

### 9.5 Publish-time gate (CI-style)

1. Manifest schema validation
2. Cedar parse + type-check + schema match
3. Cedar Symbolic self-conflict check
4. WASM static analysis
5. All `tests/cases.yaml` pass at ≥ 80% coverage
6. Property tests pass
7. Declared `permissions` ⊇ actual usage
8. Declared `requires` ⊇ actual dependencies

Gate is deterministic and reproducible; reviewers can re-run it locally and obtain the same verdict.

### 9.6 Sharing

- Playground sessions are shareable as URL-encoded blobs (or IPFS hashes).
- Fixtures are publishable as a fourth package kind; templates can attest to passing a particular fixture suite.

## 10. Wallet integration (EOA, v1)

### 10.1 Host obligations

Any wallet integrating the SDK SHALL:

1. Call the engine before any sign API, for every transaction the user is asked to sign. The engine — not the host — decides which installed policies apply; the host never pre-filters.
2. If `verdict.decision == "deny"`, refuse to sign. The wallet MUST NOT call any sign API and SHOULD show the verdict's `reason` and matched-clause diagnostics to the user.
3. If `verdict.decision == "warn"`, present the warning(s) and require explicit user confirmation before signing.
4. If `verdict.decision == "allow"`, proceed with normal signing flow.
5. Persist the verdict trace alongside the transaction record so users can later inspect why a decision was made.
6. NOT modify the transaction in any way based on the verdict.

### 10.2 Caveats

- Enforcement is at the wallet code layer. A user who exports their key to a different wallet bypasses the engine. This is documented as a limitation of the EOA model.
- ERC-4337 validator-module-based on-chain enforcement is a v2 candidate that would close this gap.

## 11. Implementation roadmap (build order — playground-first)

Schema is hard to design in the abstract. Building the playground early — even in a minimal CLI form — gives an immediate validator for every schema decision.

```
✅ v0.1  Cedar evaluator + type system + hand-written EvalContext fixtures
        Deliverable:   The four canonical swap policies (5 actually shipped)
                       evaluate correctly against hand-written fixtures.
        Status:        Pure Rust integration tests cover the use case the
                       playground was meant to provide. Browser/CLI playground
                       deferred to a later milestone.

🟡 v0.2  Manifest evaluator (decode + emit) + Uniswap V2 adapter
         + ERC-20 approve adapter
        Done:          Uniswap V2 Router02 (6 swap fns), Uniswap V3 SwapRouter
                       (5 fns incl. multicall), Universal Router (V2/V3/V4
                       command stream) — all hand-written Rust adapters.
        Pending:       Declarative YAML manifest format + manifest evaluator;
                       ERC-20 approve adapter.
        Playground:    deferred (not blocking).

✅ v0.3  Stage 3 Context Assembler + mock oracle adapter
        Status:        `MockOracle` + `enrich_with_usd` shipped; HTTP-backed
                       oracle still pending behind a future `http` feature flag.

🟡 v0.4  Adapter Registry + caching + ambiguity UX
         + Uniswap V3 adapter (single + multicall + WASM compute)
        Done:          V3 single-fn adapters + V3 multicall expansion;
                       AdapterRegistry trait is object-safe + Pipeline-generic.
        Pending:       Registry caching (TTL, snapshot ids), ambiguous-resolution
                       UX (currently surfaces ids; user-pin not implemented),
                       WASM compute escape hatch (V3 path is pure-Rust today).

⏭ v0.5  Marketplace API + packaging/signing + publish gate (lint + tests)
        Pending in full.

⏭ v1.0  Wallet integration sample + i18n + 1inch outer-only adapter
        Pending in full.
```

The playground evolves continuously from v0.1; every schema change goes through it first. **In practice, Rust integration tests have substituted for the playground so far** — fixtures are constructed in test code rather than YAML. A future `replay-fixture` CLI ships the same value with lower friction.

## 12. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Cedar lacks Korean diagnostics | Korean UX degradation | host-side `@reason` i18n; Cedar diagnostics shown as supplementary technical detail |
| WASM determinism leak | divergent verdicts | Wasmtime deterministic profile + publish-time static analysis + reject suspicious modules |
| Oracle single-source dependency | denial storms during outages | multi-source aggregation (median over Pyth + Chainlink + internal) + graceful staleness handling |
| Adapter publisher key compromise | misclassified actions bypass policies | DID-based revocation list + Stage-2 trace always attached for post-hoc audit |
| Marketplace name-squatting / impersonation | trust-model noise | reverse-DNS namespacing + reviewer attestation slider + name-similarity warnings in UI |
| Cedar expressivity gap | author frustration | manifest-side derived fields fill the gap (host pre-computes, Cedar compares) |
| EOA enforcement is advisory | user can use a different wallet to bypass | document as inherent EOA limitation; v2 adds 4337 module-based on-chain enforcement |
| Dependency hell in marketplace | broken installs | strict SemVer + integrity-hashed lockfile |
| Outer-only adapters miss inner-call risk | false-allows on aggregators | template manifests must check `analysisDepth` and warn or refuse to install |
| Stat-window persistence across devices | user loses 24h state on device switch | v1: device-local; v2: optional encrypted sync |

## 13. Open questions

1. **Approval handling and the swap-approve sequence**: with approve in v1, the engine catches both the "swap is too big" and "approval is unlimited" cases. We should validate with a real flow whether typical wallets correctly run two engine passes for the two transactions, or whether bundled UX is needed.
2. **Privacy of oracle queries**: which queries leak which transaction-intent to which oracle providers? Acceptable for Chainlink (push-style); Pyth pulls expose token identity. Mitigated by batching queries and caching.
3. **Marketplace governance**: who operates the registry, who issues the CRL, and how is multi-mirror federation specified? Open until pilot.
4. **Stat-window persistence model**: device-local (v1) or sync-able (v2)?
5. **i18n model for templates and reasons**: keys vs full strings vs gettext; choose before v1.0.

## 14. Out of scope (explicit)

- EVM simulation / fork-based price-impact computation
- MEV protection / private mempool policies
- Transaction mutation (engine returns verdicts only)
- On-chain enforcement (4337 validator modules)
- ML-based risk scoring
- Non-EVM chains
- Automatic conflict resolution between policies (manual UX only; no priority inference)

## 15. Appendix A — Adapter expression sub-language reference

```text
Identifiers and references
  $name             ABI-decoded binding from `decode.args`
  $tx.<field>       transaction context (chainId, from, to, value, data, gas, nonce)
  $params.<field>   nested struct field (Solidity tuple)

Indexing
  $arr[i]           positional index (negative allowed: $arr[-1])
  $arr[a:b]         slice
  len($arr)         length

Arithmetic (integer)
  +  -  *           ; no division or modulo
Comparisons
  ==  !=  <  <=  >  >=
Boolean
  &&  ||  !
Host capabilities (whitelisted)
  oracle.price(token)              → UsdValuation | null
  oracle.twap(tokenA, tokenB, sec) → ratio | null
  token.metadata(addr, chainId)    → TokenMetadata
  registry.protocol(id)            → ProtocolMetadata
  addrLookup(addr)                 → Address_
  tokenLookup(addr, chainId)       → Token
  tokenLookupList(addrs, chainId)  → List<Token>
  toHuman(raw, decimals)           → Decimal
  toUsdValuation(raw, valuation)   → UsdValuation
  spotConvert(raw, srcVal, dstVal) → AmountSpec
  host.uniswapV2.priceImpactBps    → Long
  host.expand.multicall            → expansion strategy
```

## 16. Appendix B — Cedar prelude (engine-provided types and helpers)

```cedar-schema
// Already defined in §4: Wallet, Token, Protocol, Address_, AmountSpec, UsdValuation, Action variants

// Standard `Op` action set
action Op = ["swap", "approve", "multi", "other", "send_tx"];

// Convenience helpers (Cedar built-ins in our prelude)
//   has "field"      — record-field-presence test
//   like             — string glob
//   .contains, .containsAll, .containsAny — set ops
//   decimal("1.23")  — Decimal literal constructor
```

## 17. Appendix C — Sample fixtures (sketch)

`fixtures/usdc-to-weth-1500usd.yaml` — see §9.2 for layout.

`fixtures/usdc-unlimited-approve-uniswap-router.yaml`:

```yaml
schemaVersion: "policy-engine/fixture/1"
tx:
  chainId: 1
  from:    "0xUser..."
  to:      "0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48"   # USDC
  value:   "0"
  gas:     "60000"
  data:    "0x095ea7b3
            0000000000000000000000007a250d5630B4cF539739dF2C5dAcb4c659F2488D
            ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
  nonce:   43
wallet:
  address: "0xUser..."
  chainId: 1
tokenRegistry:
  "1:0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48":
    symbol: "USDC"
    decimals: 6
    listings: ["uniswap-default"]
addressBook:
  "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D":
    label: "Uniswap V2 Router"
    tags: ["contract", "known-router"]
adapterPins:
  "1:0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48:0x095ea7b3": "erc20/approve@1.0.0"
engineConfig:
  failClosedDefault: true
  oracleMaxStalenessSec: 60
```

Expected verdict against `core/no-unlimited-approval`:

```
decision: deny
matchedPolicies:
  - policyId: user/no-unlimited-approval
    severity: deny
    reason: "Unlimited (max-uint) approvals are blocked"
    bindings:
      context.unlimited: true
      context.spender:   "0x7a25..."
```

---

End of v1 design.
