# Policy Engine Chrome Extension — v1 Design

| | |
|---|---|
| Status | Draft (post-brainstorm) |
| Date | 2026-05-08 |
| Scope | MVP browser extension that wraps the existing `policy-engine` Rust+Cedar runtime to evaluate dApp transaction and signature requests in real time |
| Out of scope (v1) | MetaMask Snap distribution; `wallet_sendCalls` / EIP-5792 batching; WalletConnect; multi-tx reservation coordination; pull-mode marketplace with accounts; adapter or schema extensions in user-installable bundles |

> **Reading guide.** This document specifies a Chrome extension that consumes the policy engine via a WASM bridge. The engine in `crates/` — covering DEX transaction evaluation plus v1 EIP-712 signature evaluation (Permit2, EIP-2612, EIP-712 catch-all) — is the source of truth for adapter coverage, host capabilities, and verdict shape. The extension does **not** modify engine semantics; it provides interception, fact pre-fetch, host snapshots, and user-facing UX around `Pipeline::evaluate(&Request)`.

---

## 1. Goal

Give wallet users an in-browser advisory + enforcement layer that:

1. Intercepts `eth_sendTransaction`, `eth_signTypedData{,_v3,_v4}`, `personal_sign`, and `eth_sign` calls a dApp issues to its provider (typically MetaMask).
2. Evaluates the request through the existing policy engine using user-installed Cedar policies.
3. Pass / Warn / Fail-gates the request before it reaches the underlying wallet for signing.
4. Lets users browse, install, and customize policy *presets* shipped by an external static catalog.

Non-goals in v1: replacing MetaMask, holding keys, simulating execution, multi-chain transaction sequencing.

## 2. High-level architecture

```
┌───────── Chrome Extension (Manifest V3) ─────────┐
│                                                  │
│  inpage.js     (per-tab, document_start)         │
│   ├─ wraps every EIP-1193 provider               │
│   └─ proxies handshakes via EIP-6963             │
│                                                  │
│       ▲ window.postMessage                       │
│  content-script.js  (relay, all_frames)          │
│       ▼ chrome.runtime.sendMessage               │
│                                                  │
│  background (service worker)                     │
│   ├─ orchestrator.ts (request lifecycle)         │
│   ├─ policy_engine.wasm (Rust→wasm-bindgen)      │
│   ├─ rpc-client.ts  / price-client.ts            │
│   ├─ storage.ts (chrome.storage + IndexedDB)     │
│   └─ catalog-client.ts                           │
│                                                  │
│  popup / options pages (React)                   │
│   ├─ Verdict modal (Chrome window, not popup)    │
│   ├─ Marketplace browser                         │
│   └─ Preset settings + audit log                 │
└──────────────────────────────────────────────────┘
        │                          │
        │ HTTPS                    │ HTTPS
        ▼                          ▼
   Static catalog           RPC + price API
   (GitHub Pages,           (Alchemy/Infura,
    signed index)            CoinGecko)
```

**Separation of concerns:**

- WASM is pure-functional. It receives `(Request, HostSnapshot)` JSON and returns `Verdict` JSON. No async, no network, no persistent state inside WASM.
- Orchestrator owns all async work: RPC calls, price fetches, user prompts, persistence.
- inpage.js stays minimal: gate two RPC method classes, relay everything else untouched.

## 3. Request lifecycle

### 3.1 Two-phase evaluation

The engine's host capabilities are synchronous, but browser host data (balances, allowances, prices) is async. We resolve this by splitting evaluation into ordered phases.

```
inpage.js   intercepts request                     [t=0]
   │
   ▼
content-script  relays to background               [t≈5ms]
   │
   ▼
orchestrator  enqueues pendingRequest in           [t≈10ms]
              chrome.storage.session
   │
   ▼
WASM   build_action_json(req)        → Action      [t≈30ms cold,  ≤5ms warm]
   │
   ▼
WASM   tier1_fact_plan_json(action)  → Tier1Plan   [t≈+2ms]
   │
   ▼
orchestrator  parallel fetch of Tier1Plan via      [t≈300–1500ms]
              RPC + price API
   │
   ▼
WASM   tier2_window_keys_json(...)   → WindowKeys  [t≈+2ms]
   │   (windowing keys derived from oracle USDs)
   ▼
orchestrator  fetch window snapshot from           [t≈+10ms]
              chrome.storage.local
   │
   ▼
WASM   evaluate_json(req, snapshot)  → Verdict     [t≈10–80ms]
   │
   ▼
orchestrator  enforce: Pass passthrough,
              Warn open Chrome window,
              Fail return JSON-RPC 4001
```

### 3.2 Latency budget (fail-closed)

| Stage | Target | Hard timeout |
|-------|--------|--------------|
| WASM cold compile | ≤300ms | 1s |
| WASM warm round-trip (3 calls) | ≤200ms | 500ms |
| Host fact fetch (RPC + price) | ≤1.5s p95 | 2.5s |
| End-to-end intercept → verdict | ≤2s p95 | **3s hard** |

If the hard timeout fires, the orchestrator opens a Warn-style modal with a "cannot evaluate — proceed anyway?" message. **Default action is reject.** Silent passthrough on timeout is forbidden.

### 3.3 Engine API additions required

The current `Pipeline` exposes `evaluate(&Request)`, `evaluate_tx`, `evaluate_sig`, `evaluate_with_reservation`. Internally it dispatches through two private builders — `build_action(&TransactionRequest)` and `build_signature_action(&SignatureRequest)` — and lowers via the public `request_from_action_with_host`. The extension needs a **public action-building surface** plus a **fact-plan surface** to drive prefetch:

```rust
// crates/policy-engine/src/pipeline.rs — new public dispatching wrapper
impl<R: AdapterRegistry + ?Sized> Pipeline<'_, R> {
    /// Build the semantic Action for a request without enrichment, lowering,
    /// or evaluation. Wraps the existing private builders for Tx and Sig.
    pub fn build_action_for(&self, request: &Request) -> Result<Action, PipelineError>;
}

// crates/policy-engine/src/lowering/mod.rs — new fact-plan module
pub struct HostFactPlan {
    pub tokens_for_oracle:        Vec<Token>,
    pub balances:                 Vec<(Address, Token)>,
    pub allowances:               Vec<(Address, Token, Address)>,
    pub clock_required:           bool,
    pub sig_oracle_requirements:  Vec<OracleRequirement>,
}
pub struct WindowKeyPlan { pub keys: Vec<WindowKey> }

pub fn required_host_facts(action: &Action) -> HostFactPlan;
pub fn required_window_keys(action: &Action, oracle: &OracleSnapshot) -> WindowKeyPlan;
```

`required_host_facts` derives Tier-1 demand from a bare Action: for `Action::Dex` it walks `DexFacts.inputs`/`outputs`, `oracle_requirements`, and the (`actor`, `token`, `target`) tuples that `enrich_dex_action_base` will need; for the three signature actions it surfaces `OracleRequirement`s and the spender/owner/token tuples used by signature stamping.

The two-tier shape is forced by `compute_dex_window_deltas` depending on already-enriched `total_input_usd` (Oracle output). Tier 1 plan is derivable from the bare Action; Tier 2 plan (window keys) needs an Oracle snapshot first.

The engine already validates EIP-712 typed data at the top of `evaluate_sig` via `validate_typed_data` — extension code never has to pre-validate. Lowering or validation errors surface as `PipelineError::Lowering` and the WASM boundary forwards them to the orchestrator's fail-closed path (§4.3.2).

This is an engine PR, not extension code, and lands before the WASM build.

## 4. Browser-side components

### 4.1 inpage.js — provider wrapping

- Injected at `document_start` via content script (the manifest sets `world: "MAIN"` in MV3).
- Listens for `eip6963:announceProvider` events; for each announcement, wraps the provided `EIP1193Provider` and re-announces under our own UUID (separate identity, *not* impersonation).
- Also wraps the legacy `window.ethereum` reference if present.
- Gated methods (must round-trip to the engine before reaching the underlying provider):
  - `eth_sendTransaction`
  - `eth_signTypedData`, `eth_signTypedData_v3`, `eth_signTypedData_v4`
  - `personal_sign`, `eth_sign`
- Passthrough + log:
  - `eth_sendRawTransaction` — already-signed bytes; v1 logs and shows a non-blocking advisory toast ("transaction submitted via raw path, not evaluated"). v1.1 adds alloy-based recovery + decode.
- Out of scope (v1): `wallet_sendCalls` (EIP-5792), WalletConnect.

**Known limitation — EIP-6963 race.** Re-announcing a wrapped provider does not deterministically beat MetaMask. Some dApps may cache the original. v1 documents this in the user-facing FAQ; v1.1 ships a MetaMask Snap as a parallel path that mediates at the wallet layer instead of the page layer.

### 4.2 content-script.js — message relay

Stateless relay. Forwards `policy:request` and `policy:verdict` envelopes between inpage and background via `chrome.runtime.sendMessage`. No business logic.

### 4.3 background (service worker)

#### 4.3.1 Lifecycle

MV3 service workers can be terminated after ~30s idle. The orchestrator treats every in-flight request as potentially surviving SW death:

- `chrome.storage.session`: live request queue (`pendingId → minimal envelope`). Cleared on browser restart, fast access. Body redacted to keep size below the 10MB session quota.
- IndexedDB: full request body (typed data can exceed 100KB) keyed by `pendingId`.
- `chrome.storage.local`: audit log (verdict + matched policies; raw calldata redacted by default, opt-in to retain).
- WASM cached as `WebAssembly.Module` in IndexedDB after first compile; on SW wake, restored via `structuredClone` before instantiation.

The Warn modal opens via `chrome.windows.create({type: "popup"})` — a separate browser window, not the action popup. This window holds an open port to the SW; while the port is connected, the SW stays alive.

#### 4.3.2 WASM boundary

One JSON-string boundary. Policies are installed once per session and held inside the WASM module; per-request calls exchange only the request, plans, and snapshots:

```rust
// session lifecycle
#[wasm_bindgen] pub fn install_policies_json(policies_json: &str) -> Result<(), JsError>;

// per request, in order
#[wasm_bindgen] pub fn build_action_json(req_json: &str)         -> Result<String, JsError>;
#[wasm_bindgen] pub fn tier1_fact_plan_json(action_json: &str)   -> Result<String, JsError>;
#[wasm_bindgen] pub fn tier2_window_keys_json(
    action_json: &str,
    oracle_snapshot_json: &str,
) -> Result<String, JsError>;
#[wasm_bindgen] pub fn evaluate_json(
    req_json: &str,
    host_snapshot_json: &str,
) -> Result<String, JsError>;
```

`install_policies_json` is called on SW wake (after WASM module restore) with the rendered text of all installed bundles' active policies, namespaced as `<bundle_id>::<policy_name>`. Updates from install/uninstall/parameter-edit invoke it again with the new set.

JSON over the boundary avoids `serde-wasm-bindgen` quirks for types that don't derive `Serialize`/`Deserialize` (e.g., `Verdict`). The Rust side wraps existing types into JSON-friendly DTOs at the boundary; engine internals don't need new derives.

**Fail-closed at the boundary.** Malformed JSON, panic, or non-`Verdict` output is treated as `Verdict::Fail` with reason `engine_unreachable`. `PipelineError::Lowering` (e.g. malformed EIP-712 typed data, missing `primaryType`, type cycle, non-decimal token amounts — caught by the engine's `validate_typed_data` and decimal helpers) surfaces as `Verdict::Fail` with reason `lowering_rejected` and the underlying error string forwarded to the verdict modal. `PipelineError::Ambiguous` becomes `Verdict::Fail` with reason `adapter_ambiguous`. Never passthrough on any engine error.

#### 4.3.3 Pending-deltas queue (windowing without reservations)

MVP does **not** use `evaluate_with_reservation`. The reservation model is hard to settle in a browser where tx outcomes are async and unreliable. Instead:

- `evaluate_tx` projects window deltas inline via `compute_dex_window_deltas` — already the correct shape.
- Orchestrator maintains a `pendingDeltas: Map<txHash | requestId, DeltaSet>` keyed by request id.
- When the user signs (Pass/Warn-confirm), the entry transitions to `pendingByTxHash` once MetaMask returns the hash.
- `chrome.alarms` polls receipts every 30s. On confirmation: commit to `chrome.storage.local` window state. On 5-minute idle without a hash (user closed wallet, dropped from mempool): discard.
- StatWindows snapshot fed into evaluation = `committed + sum(pendingDeltas.values())`.

This blocks the Codex-flagged concurrent-pending-tx cap-bypass: two simultaneous swaps each see the other's pending delta in the snapshot.

### 4.4 popup / options

- **Verdict modal**: shown on Warn. Lists matched policies with severity icons and reasons. Two buttons: *Cancel* (reject 4001), *Trust and proceed*. Trust-and-proceed forwards the original RPC call to the underlying provider unchanged.
- **Marketplace**: lists bundles fetched from the static catalog. Each bundle shows: id, version, author, signing key fingerprint, summary, parameters with defaults. Install / update / uninstall.
- **Preset settings**: per installed bundle, parameter form rendered from `params.schema.json`. Policies inside the bundle can be individually toggled.
- **Audit log**: last N verdicts (default 100). Filters by chain, action type, severity.

## 5. Marketplace

### 5.1 Catalog format

Static, fetched over HTTPS. No backend, no accounts.

```
https://catalog.example/index.json
{
  "catalog_version": 1,
  "signed_at": "2026-05-08T...",
  "signature": "ed25519:...",
  "publisher_pubkey": "ed25519:...",
  "bundles": [
    { "bundle_id": "...", "version": "1.0.0",
      "manifest_url": "...", "manifest_sha256": "..." },
    ...
  ]
}
```

The extension ships with the publisher root key pinned. Index signature verification fails closed; bundle hashes verified before install.

### 5.2 Bundle layout

```
bundle.zip
├── manifest.json
├── policies/
│   ├── max-input-usd.cedar.tmpl
│   └── allowlist-spenders.cedar.tmpl
└── params.schema.json
```

`manifest.json`:

```json
{
  "manifest_version": 1,
  "bundle_id": "uniswap-conservative",
  "version": "1.2.0",
  "author": { "name": "...", "pubkey": "ed25519:..." },
  "policies": [
    { "id": "max-input-usd", "file": "policies/max-input-usd.cedar.tmpl", "default_severity": "deny" },
    ...
  ],
  "params_schema": "params.schema.json"
}
```

### 5.3 Bundle sandbox (v1 invariant)

A bundle is exactly two artifact classes: **Cedar policy templates** + **typed parameter schema**.

Forbidden in v1 bundles:
- Cedar schema fragments (no new entity types, context fields, action ids)
- Adapter code (no new calldata decoders)
- Host capability extensions
- Arbitrary scripts / WASM blobs / resources beyond static metadata files

Install validation rejects any zip containing files outside the allowed list. The engine receives only rendered policy text — never schema text — from a bundle.

This invariant prevents the "non-firing forbid" attack class: bundles cannot manipulate context production, schema, or host-fact availability, so they cannot weaken existing user `forbid` policies.

### 5.4 Future extensibility

`manifest_version` gates which artifact classes are allowed. Future versions can extend safely:

| Manifest version | Additional artifacts | Trust requirement |
|------------------|----------------------|-------------------|
| **1 (MVP)** | — | publisher signature only |
| 2 | `schema_extensions/*.cedarschema` | + user opt-in per install |
| 3 | declarative `adapter_capabilities` | + extension publisher review |

An older extension always rejects newer manifest versions outright, so users never get downgraded into an unsafe sandbox.

### 5.5 Trust threats and mitigations

| Threat | Mitigation |
|--------|------------|
| Catalog MITM / GitHub Pages compromise | Pinned root key in extension; signature on `index.json`; sha256 on bundle |
| Typo-squatted bundle | Author pubkey shown at install; first-install pubkey pinned per `bundle_id`, future updates must match |
| Malicious bundle weakens existing forbids | §5.3 sandbox + Cedar `forbid`-wins semantics + `policy_id` namespacing (`<bundle_id>::<name>`) |
| Catalog publisher key compromise | Out of scope for v1; v1.1 adds key rotation manifest |

## 6. Parameterization

### 6.1 The substitution attack

Plain text `{{x}}` substitution into Cedar source is unsafe. A string param can break out of a string literal (`"\" } } forbid (...) when { true } //`) and inject new policies; a numeric param without bounds checking can introduce expressions; a `when {false}` injection can neuter an existing forbid without changing the policy head count.

### 6.2 Defense in depth

**Layer 1: Typed parameter schema (`params.schema.json`).** Each parameter declares a strict type:

```json
{
  "max_input_usd": { "type": "integer", "min": 1, "max": 10_000_000 },
  "allowed_spenders": { "type": "array", "items": { "type": "address" }, "maxItems": 64 },
  "venue": { "type": "enum", "values": ["uniswap_v2", "uniswap_v3"] }
}
```

Supported types: `integer{min,max}`, `address` (`^0x[a-fA-F0-9]{40}$`), `enum`, `string{maxLen,allowedChars}`, `array{items, maxItems}`. No free-form expressions, no booleans-as-text, no nested objects.

**Layer 2: Typed Cedar literal serializer.** Per-type renderer constructs Cedar literals from validated values:

- `integer` → `^-?[0-9]+$` regex re-checked, rendered raw.
- `address` → rendered as Cedar `EthereumAddress::"0x…"` entity literal.
- `enum` → mapped to a closed Cedar entity per enum domain.
- `string` → escape `\\`, `"`, control chars; wrap in `"..."`; assert length cap.
- `array` → rendered as Cedar `Set` of pre-serialized typed elements.

**Layer 3: AST-equivalence post-render check.** Templates are parsed once at load to identify *parameter slot positions* (the AST node coordinates that templates designate as substitution sites). After rendering and re-parsing, the runtime walks both ASTs and asserts they are identical *except* at slot coordinates, where only the typed value at the slot may differ.

This catches `when { false }`, `unless { true }`, scope changes, severity annotation changes, head edits, etc. — all attacks that preserve `permit`/`forbid` head count but mutate non-slot subtrees.

If `cedar-policy`'s public AST surface is insufficient to do node-level walks, fallback is to construct Cedar policy ASTs directly via a small in-extension builder rather than rendering text at all.

## 7. Storage layout

| Store | Contents | Quota concern |
|-------|----------|---------------|
| `chrome.storage.session` | Pending request envelope (id, chain, method, redacted summary). | 10MB; bodies kept in IndexedDB |
| `chrome.storage.local` | Audit log, window state, installed bundle metadata, RPC URL prefs, user toggles | 10MB; rotated/capped |
| IndexedDB | Full request bodies; compiled `WebAssembly.Module`; rendered policy text per installed bundle | Soft quota multi-GB |
| In-memory (SW) | Live evaluation state; ephemeral | Lost on SW shutdown — every recovery path must rebuild from persistent stores |

Audit log redaction is on by default: `to`, `chainId`, action class, verdict, matched policy ids. Raw calldata / typed data only retained if user opts in. This keeps wallet activity from being logged at rest.

## 8. Security model summary

| Attack | Defense | Residual risk |
|--------|---------|---------------|
| dApp bypasses proxy via raw provider | EIP-6963 wrap + `document_start` + `all_frames` | Best-effort — race with MetaMask injection. Mitigated by v1.1 Snap. |
| Malicious bundle injects policy code | §6 typed serializer + AST equivalence + bundle sandbox | Depends on Cedar AST API completeness; fallback is direct AST construction |
| Catalog tampering | Pinned root key, signed index, sha256 on bundles | Root key compromise out of scope v1 |
| WASM panic / corruption | JSON boundary fail-closed; engine-unreachable verdict = Fail | Operational (alarming/telemetry) v1.1 |
| Concurrent pending-tx cap bypass | Pending-deltas queue with TTL, included in window snapshot | Drop-without-receipt edge cases (5-min TTL) |
| `eth_sendRawTransaction` | Advisory toast, log; not evaluated in v1 | Known gap; closes in v1.1 |

## 9. Out of scope (future)

- MetaMask Snap parallel path (target v1.1) — closes the EIP-6963 determinism gap
- `wallet_sendCalls` / EIP-5792 batch evaluation
- WalletConnect bridge interception
- Multi-tx reservation coordination (revives `evaluate_with_reservation`)
- Marketplace v2: schema-extension bundles (manifest_version=2)
- Marketplace v3: declarative adapter bundles (manifest_version=3)
- Catalog publisher key rotation
- Firefox / Safari ports

## 10. Testing strategy

- **Unit**: WASM boundary serialization, parameter serializer per type, AST equivalence checker (positive + injection corpus), pending-deltas queue under SW restart simulation.
- **Integration**: full intercept-to-verdict path against a fixture dApp; SW kill-and-restart while a Warn modal is open; multi-pending-tx cap behavior.
- **Browser**: Playwright against MetaMask preview build for EIP-6963 ordering; manual matrix on top 10 dApps for proxy coverage.
- **Engine**: existing `crates/integration-tests` continues to cover `Pipeline::evaluate*`. New tests cover `build_action` and `required_host_facts` shapes.

## 11. Repository layout (proposed)

```
policy-engine/
├── crates/
│   └── policy-engine/             # existing; gets build_action + required_host_facts
├── extension/                     # new
│   ├── manifest.json
│   ├── inpage/
│   ├── content-script/
│   ├── background/
│   │   ├── orchestrator.ts
│   │   ├── wasm-bridge.ts
│   │   ├── rpc-client.ts
│   │   ├── price-client.ts
│   │   ├── catalog-client.ts
│   │   └── storage.ts
│   ├── ui/                        # popup + options pages
│   └── wasm/
│       └── policy_engine_wasm/    # cargo crate, wasm-bindgen front-end
└── catalog/                       # static site source for the marketplace
    ├── index.json (signed)
    └── bundles/
```
