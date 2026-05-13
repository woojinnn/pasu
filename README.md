# policy-engine

A Rust/Cedar reference implementation for wallet-side transaction and
signature policy evaluation. The current v0.x demo covers EVM DEX
transactions plus v1 EIP-712 signature evaluation for Permit2, EIP-2612, and
unmatched typed data:

1. A registry resolves a calldata adapter.
2. The adapter emits one semantic `Action`.
3. The pipeline enriches that action with host-provided facts.
4. Lowering builds one Cedar `PolicyRequest`.
5. Cedar evaluates the request against the installed policy set.

The public policy surface is intentionally coarse. DEX routers, multicalls, and
Universal Router command streams are aggregated into `Action::Dex`; unknown or
unsupported calls become `Action::Other`. Signature requests use separate Cedar
action ids: `signature.permit2`, `signature.eip2612`, and
`signature.eip712_other`.

## Workspace Layout

```text
policy-engine/
|-- Cargo.toml
|-- README.md
|-- docs/
|   `-- specs/
|-- policy-schema/
|   |-- core.cedarschema
|   `-- actions/
|       |-- dex.cedarschema
|       |-- eip2612.cedarschema
|       |-- eip712_other.cedarschema
|       |-- other.cedarschema
|       `-- permit2.cedarschema
|-- policies/
|   |-- dex/
|   `-- signature/
|       |-- eip2612/
|       |-- eip712-other/
|       `-- permit2/
`-- crates/
    |-- policy-engine/
    |-- adapters/
    |   |-- eip2612/
    |   |-- permit2/
    |   |-- uniswap-v2/
    |   |-- uniswap-v3/
    |   `-- universal-router/
    |-- adapters-bundle/
    `-- integration-tests/
```

## Runtime Crate

`crates/policy-engine` contains the core runtime:

- `core.rs`: `Address`, `Token`, `TransactionRequest`, `SignatureRequest`,
  `Request`, `Action`, `DexAction`, `DexFacts`, `OracleRequirement`,
  `OtherAction`, and signature action types.
- `adapter.rs`: action adapter SDK surface. Transaction adapters implement
  `TransactionActionAdapter` or `DeclaredTransactionActionAdapter`; signature
  adapters implement `SignatureActionAdapter` or `DeclaredSignatureActionAdapter`.
- `registry.rs`: transaction lookup by `(chain_id, to, selector)` and
  signature lookup by `(chain_id, verifying_contract, primary_type)`.
- `host/`: host capability traits and mocks:
  `Oracle`, `Clock`, `Portfolio`, `Approvals`, `StatWindows`.
- `lowering/`: DEX/signature enrichment and `Action -> PolicyRequest`
  conversion.
- `policy.rs`: Cedar wrapper, schema validation, `Verdict`, `MatchedPolicy`.
- `pipeline.rs`: resolver -> adapter -> enrichment -> lowering -> Cedar.
- `schema.rs`: `PolicySchemaComposer`, which loads `policy-schema/*`.
- `context_keys.rs`: centralized Cedar context field names used by lowering.

`context_keys.rs` is the engine-side Cedar context vocabulary. Adapter authors
should normally target the Rust core types (`DexFacts`, `OracleRequirement`,
etc.), not those string constants directly. Lowering owns the mapping from Rust
fields to Cedar context keys.

## Action Model

```rust
pub enum Action {
    Dex(DexAction),
    Other(OtherAction),
    Permit2(Permit2Action),
    Eip2612(Eip2612Action),
    Eip712Other(Eip712OtherAction),
}
```

`Action::Dex` represents the whole transaction-level DEX intent. A Uniswap V3
multicall or Universal Router execution still produces one DEX action, not a
list of policy-evaluated leaf actions.

`DexAction` carries:

- `actor`, `target`, and `value_wei`
- aggregate `DexFacts`
- `oracle_requirements` for host valuation
- `trace` for debugging/audit output outside Cedar context

`Action::Other` is emitted when no adapter matches. It gives policies a stable
surface for unknown calls without pretending they are DEX activity.

Signature adapters emit `Action::Permit2` and `Action::Eip2612`. The
EIP-712 catch-all is a pipeline fallback only: when no signature adapter
matches, `Pipeline::evaluate(&Request::Sig(...))` builds `Action::Eip712Other`
directly.

## Cedar Request Shape

The composed schema is:

```text
policy-schema/core.cedarschema
policy-schema/actions/dex.cedarschema
policy-schema/actions/other.cedarschema
policy-schema/actions/permit2.cedarschema
policy-schema/actions/eip2612.cedarschema
policy-schema/actions/eip712_other.cedarschema
```

DEX requests evaluate:

```cedar
action == Action::"dex"
resource is Protocol
context is DexContext
```

`DexContext` contains aggregate primitives:

- `target`, `valueWei`
- `protocolIds`
- `inputTokens`, `outputTokens`
- `totalInputUsd`, `totalMinOutputUsd`
- `maxFeeBps`
- `hasZeroMinOutput`
- `hasExternalRecipient`
- `totalInputFractionOfPortfolioBps`
- `allowancesCoverInputs`
- `windowStats`

The shipped policies under `policies/dex/` validate against the composed schema
in `crates/integration-tests/tests/schema_validation.rs`.

Signature requests evaluate:

```cedar
action == Action::"signature.permit2"
action == Action::"signature.eip2612"
action == Action::"signature.eip712_other"
resource is Protocol
```

Signature context includes the signer, request/domain chain ids, verifying
contract, primary type, host-clock `nowTs`, deadline deltas, nonce sanity,
spender/verifying-contract allowlist fields, token amount fields, and optional
`totalApprovedUsd` where an oracle price was available.

## Adapter Model

Adapters decode protocol calldata and emit semantic actions. They do not emit
Cedar `PolicyRequest`s directly and they do not attach policy-schema fragments.

Currently shipped adapters:

- Permit2 EIP-712 PermitSingle, PermitBatch, and PermitTransferFrom
- EIP-2612 Permit typed data
- Uniswap V2 Router02 swap functions
- Uniswap V3 SwapRouter exact-input/exact-output functions
- Uniswap V3 multicall
- Uniswap Universal Router V2/V3/V4 swap command extraction

`crates/adapters-bundle` exposes `default_registry()` for transaction adapters
and `default_signature_registry()` for Permit2/EIP-2612 signature adapters.
The catch-all EIP-712 branch is intentionally not registered there.

Adding an internal adapter:

1. Create `crates/adapters/<name>/`.
2. Implement `DeclaredTransactionActionAdapter` or `TransactionActionAdapter`.
3. Return `Action::Dex` for supported DEX calldata or `Action::Other` if the
   adapter intentionally models an unknown/non-DEX action.
4. Register it in `crates/adapters-bundle`.
5. Add unit and integration coverage.

## Host Capabilities

`HostCapabilities` is the boundary between wallet host data and the policy
engine. The demo uses mock providers, but production implementations can slot
behind the same traits.

- `Oracle`: token -> USD unit price. Used to stamp `totalInputUsd` and
  `totalMinOutputUsd`, and signature `totalApprovedUsd`.
- `Clock`: current Unix timestamp. Used to stamp signature deadline deltas.
  `HostCapabilities::new(&oracle)` defaults this to `SystemClock`; tests can
  use `with_clock(&MockClock::with_fixed(...))`.
- `Portfolio`: `(owner, token) -> balance`. Used to stamp
  `totalInputFractionOfPortfolioBps`.
- `Approvals`: `(owner, token, spender) -> allowance`. Used to stamp
  `allowancesCoverInputs`.
- `StatWindows`: per-owner rolling counters with reserve/settle/release.
  Used to stamp projected `windowStats`.

Missing providers or missing records are fail-open for this demo: the field is
omitted, and policies are expected to guard with `context has <field>`.

## Evaluation Flow

```text
Pipeline::evaluate(&Request)
|-- Request::Tx(TransactionRequest)
|   |-- TransactionActionAdapterRegistry::resolve_with_adapter
|   |-- TransactionActionAdapter::build_action or Action::Other
|   |-- DEX host enrichment, if Action::Dex
|   |-- lowering::request_from_action
|   `-- Cedar action ids: dex, other
|
`-- Request::Sig(SignatureRequest)
    |-- SignatureActionAdapterRegistry::resolve
    |-- SignatureActionAdapter::build_action, if Permit2/EIP-2612 matched
    |-- Action::Eip712Other fallback, if unmatched
    |-- signature host enrichment with Oracle + Clock
    |-- lowering::request_from_action_with_host
    `-- Cedar action ids: signature.permit2, signature.eip2612,
        signature.eip712_other

Both branches:
  -> PolicyEngine::evaluate_requests(... PolicyRequestOrigin::Action ...)
  -> Verdict
```

`Pipeline::evaluate_with_reservation` reserves projected DEX stat-window deltas
before evaluation, so cap policies see the post-this-transaction window state.
If Cedar returns `Verdict::Fail`, the speculative reservation is released.

## Verdict Shape

```rust
pub enum Verdict {
    Pass,
    Warn(Vec<MatchedPolicy>),
    Fail(Vec<MatchedPolicy>),
}

pub struct MatchedPolicy {
    pub policy_id: String,
    pub reason: Option<String>,
    pub severity: Severity,
    pub origin: PolicyRequestOrigin,
}

pub enum PolicyRequestOrigin {
    Action,
    Tx,
}
```

`PolicyRequestOrigin` is not the same thing as the host-facing `Request::Tx` /
`Request::Sig` input enum. It records which Cedar request layer produced a
matched policy. The current pipeline lowers each evaluated transaction or
signature to action-level Cedar and therefore reports
`PolicyRequestOrigin::Action`. `PolicyRequestOrigin::Tx` is reserved for a
future raw transaction-level Cedar request, if the engine adds one alongside
the semantic action request.

## Running

```bash
cargo test
cargo run -p policy-engine-adapters-bundle --example e2e_swap
```

### Docker dev environment

A reproducible toolchain (Rust 1.83 + wasm-pack + Node 20 + Yarn 4 + headless Chromium) is available via Docker Compose so contributors don't need to manage the polyglot setup locally.

```bash
./scripts/dev-up.sh      # build the image, start the container, drop into a shell
./scripts/test-all.sh    # run the full test/lint/build sweep inside the container
./scripts/wasm-build.sh  # rebuild the wasm artifact and copy it into extension/
```

Persistent volumes keep cargo, yarn, and `extension/node_modules` caches alive across `docker compose down`. Wipe with `docker compose down -v`. See `docker/README.md` for the full layout.

### CI

`.github/workflows/ci.yml` runs the same test/lint/build sweep on every PR:

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo doc -D warnings`
- `wasm-pack build` plus headless wasm-bindgen tests
- Extension typecheck, vitest, Chrome MV3 build, Firefox MV2 build
- Both browser zips uploaded as PR artifacts so reviewers can sideload without recompiling
- `cargo audit` and `cargo deny` for supply-chain checks

Example output:

```text
─── 50 USDT (under cap) ───
  verdict  : Pass

─── 100 USDT (at cap) ───
  verdict  : Pass

─── 200 USDT (over cap) ───
  verdict  : Fail
  matched  : user/max-input-usd-100 USD value of Dex input exceeds 100
```

## Test Coverage

The workspace currently has 229 passing tests plus 1 ignored doctest:

| Area | Tests | Coverage |
|---|---:|---|
| `policy-engine` unit tests | 58 | core types, host mocks, decimal helpers, DEX enrichment, schema validation, policy evaluation, registry |
| Uniswap V2 adapter | 24 | selector pins, ABI round trips, DEX action construction |
| Uniswap V3 adapter | 41 | per-function encode/decode, path decoding, multicall aggregation, ABI cross-checks |
| Universal Router adapter | 5 | execute decoding and aggregate DEX action construction |
| Integration tests | 56 | adapter-to-request lowering, capabilities, DEX policies, schema validation, window stats, unknown calls |

## Deliberately Not Here Yet

- ERC-20 approve/transfer adapters
- 1inch, CowSwap, Curve, Balancer, Pendle adapters
- RPC/HTTP-backed production capability providers
- Manifest-driven adapter loading
- Lazy capability planning
- Marketplace packaging/signing
- Playground UI
- Cedar symbolic conflict checks
