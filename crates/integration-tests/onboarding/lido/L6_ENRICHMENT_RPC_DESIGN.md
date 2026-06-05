# DESIGN — Wiring the v2 Enrichment Pipeline: server-side `POST /v1/rpc` fetcher route

**Status:** Design (decision-ready, not implementation)
**Audience:** ScopeBall maintainer + woojinnn (owner of the axum `policy-server`)
**Date:** 2026-06-03
**Branch context:** `feat/lido-onboarding` (worktree `scopeball-lido`); Lido decoder slice landed in `decoder.rs` at commit `03de73e3`. This doc is the **L6(B)** deliverable; **L6(A)** = the decoder slice. Cross-ref: `LIMITATIONS.md` §L6.

---

## 1. Summary

ScopeBall's stateless **v2 enrichment / policy-RPC pipeline** is built end to end on the **client/WASM side** but **dormant in production** because it has no server to talk to. A manifest's `policy_rpc[]` enrichment specs are planned by WASM (`plan_action_rpc_v2_json`), the extension SW POSTs the resolved batch to `${POLICY_RPC_URL}/v1/rpc` (`postPolicyRpc`, `policy-rpc.ts:382-421`), and WASM folds results back into Cedar `context.custom.*` (`materialize_v2.rs`). **Every hop exists except one: the server route that actually receives the batch, executes the reads, and returns results.** `app.rs` has **no `/v1/rpc` route**, `/evaluate` stubs the results map to empty, and `config.rs` has no RPC-provider URL. The single deliverable is a **fetch-plus-decode-only** `POST /v1/rpc` handler on the policy-server that resolves each planned `(method, params)` against a server-side registry whitelist, performs `eth_call`s via the existing `OnchainViewFetcher`, decodes by `decoder_id`, and echoes `{request_id, results[]}`. Default config keeps it disabled; when a `POLICY_RPC_*` URL is configured, enrichment goes live. **Verdicts stay 100% local in WASM Cedar — the server returns data, never a verdict.**

---

## 2. Pipeline status table — 8 hops

| # | Hop | File / fn | Status |
|---|-----|-----------|--------|
| 1 | Manifest declares `policy_rpc[]` enrichment spec (`id`, `method`, `params` template, `outputs`, `optional`, `ttl_s`) | `crates/policy-engine/src/policy_rpc/manifest_v2.rs:117-132` | **WIRED** |
| 2 | WASM plans calls: resolve `$.`-selectors → structured-JSON `params`, mint `call_id = manifest_id::spec_id` | `crates/policy-engine/src/policy_rpc/planning_v2.rs:27-50,84-122`; export `plan_action_rpc_v2_json` (`action_eval_exports.rs:118-132,337-342`) | **WIRED** |
| 3 | SW builds batch (`{id,method,params}` per call), drops all-local/math methods, mints `request_id = "action-v2:" + firstCall.id` | `policy-rpc.ts:308-349`; local short-circuit `local-method-handlers.ts:131-154` | **WIRED** |
| 4 | SW POSTs `${POLICY_RPC_URL}/v1/rpc` (`content-type: application/json`), HTTP-2xx + `request_id`-echo + `Array.isArray(results)` validation | `policy-rpc.ts:382-421` | **WIRED** |
| 5 | **Server receives batch** | — | (boundary) |
| **6** | **Server executes each call (`eth_call`), decodes by `decoder_id`, returns `results[]`** | **`app.rs` — NO `/v1/rpc` route (`app.rs:146-188`); `/evaluate` stubs empty results map; `config.rs` has no RPC URL** | **ABSENT — THE GAP** |
| 7 | SW folds: `entry.id` string + `entry.ok===true` → `map[id]=entry.result`; failures/malformed omitted (fail-closed) | `policy-rpc.ts:366-378` | **WIRED** |
| 8 | WASM materializes: lookup by `call_id`, apply `outputs[].from = "$.result.*"` → write `context.custom.*`; missing **required** → `SystemFail` → `__system__` fail-closed verdict | `crates/policy-engine/src/policy_rpc/materialize_v2.rs:31-62,97,123` | **WIRED** |

> Reusable machinery already exists and is **untouched by this gap**: `OnchainViewFetcher` (`sync/src/sources/fetchers/onchain.rs:55-174`) and the Lido decoder slice (`decoder.rs`, commit `03de73e3`). Today only wallet-state sync uses the fetcher.

---

## 3. The `/v1/rpc` contract

### 3.1 Request (server MUST accept)

`PolicyRpcBatchRequestDto` (`wasm-bridge.types.ts:12-26`), `JSON.stringify(plan)`:

```json
{
  "request_id": "action-v2:<first-call-id>",
  "calls": [
    { "id": "<manifest_id>::<spec_id>", "method": "<opaque-name>", "params": { "...": "..." } }
  ]
}
```

- **`request_id`** — `"action-v2:" + (remoteCalls[0]?.id ?? "calls")` (`policy-rpc.ts:345`). Server **MUST echo it verbatim** in the response or the SW rejects the body (`policy-rpc.ts:398`).
- **`calls[].id`** = the `call_id` `"<manifest_id>::<spec_id>"` (`planning_v2.rs:44-50`). Server **MUST echo this exact `id`** on each result entry; it is the map key.
- **`calls[].method`** = an **OPAQUE remote method NAME**, e.g. `oracle.usd_value`, `chain.is_contract` (`planning_v2.rs:33-34`; doc-comment `manifest_v2.rs:121-122`). **NOT** a Solidity signature, **NOT** a 4-byte selector. It is whatever string the manifest author wrote.
- **`calls[].params`** = a **fully-resolved structured-JSON object** (`serde_json::Value::Object`), selectors already substituted (`planning_v2.rs:84-122`). **NOT** ABI-encoded hex, **NOT** a `0x` calldata blob. Example resolved object (`planning_v2.rs:194-201`): `{ "chain_id": "eip155:1", "recipient": "0xrecipient", "static": "literal" }`. Scalars are decimal strings / numbers / plain-string hex addresses.

> Only calls **not** answered locally appear here. Pure-math methods (`token.normalize_to_nano`) are computed in-process and **never POSTed** (`policy-rpc.ts:324-341`). An empty/all-local batch produces **no POST at all** (`policy-rpc.ts:313,335-341`).

### 3.2 Response (server MUST return)

HTTP **2xx** (non-2xx → SW throws `policy-rpc returned HTTP <status>`, `policy-rpc.ts:394-396`). Body = `PolicyRpcResponseDto`:

```json
{
  "request_id": "action-v2:<first-call-id>",
  "results": [
    { "id": "<manifest_id>::<spec_id>", "ok": true,  "result": { "usd": 3500 } },
    { "id": "<manifest_id>::<spec_id>", "ok": false, "error": { "code": "...", "message": "..." } }
  ]
}
```

- **Top-level validation** (`policy-rpc.ts:398-400`): body rejected unless `request_id === plan.request_id` **AND** `Array.isArray(results)`.
- **Success entry** `{ id, ok:true, result }` → folded as `map[id] = result`. `result` is the **UNWRAPPED projection root** (what `$.result.*` selectors read), **NOT** a re-nested `{ok,result}` envelope (`policy-rpc.ts:366-378`). For an `oracle.usd_value`-style call the server returns e.g. `{ "usd": <num> }` and the projection reads `$.result.usd` (`ContextProjection.from`, `manifest.rs:116-129`).
- **Failure / malformed entry** (`ok !== true`, or non-object, or `id` not a string) → **OMITTED** from the map (`policy-rpc.ts:367-377`). Mirror shape documented at `local-method-handlers.ts:25-31`.
- **Fail-closed consequence:** an omitted result for a **required** (`optional:false`) call → WASM `materialize_v2` `SystemFail` → `__system__` fail-closed verdict (`materialize_v2.rs:52-62`). The server must return `ok:true` **only** when it genuinely fetched the value; a wrong/uncertain value should be `ok:false`/omitted (which blocks, never waves through).

### 3.3 How the server turns `(method, params)` into an `eth_call` + which `decoder_id`

**Load-bearing finding:** the v2 wire carries **no implicit `to`/`data`**. There is no generic `(to, abi, args) → calldata` contract on the wire. The server therefore **cannot** build an `eth_call` from `params` generically — it **must own per-method knowledge**. Two valid mechanisms, in priority order:

1. **Registry-keyed resolution (whitelist, REQUIRED — see §5).** The server holds a **server-side mirror of the registry call-specs** keyed by `call_id = manifest_id::spec_id` (and/or by `method`). For each incoming call it looks the spec up, and **reconstructs `(chain, contract, function/selector, decoder_id, ABI-typed args)` from the trusted manifest**, reading the concrete arg values out of the structured `params` object by the manifest's declared param names. It then drives `OnchainViewFetcher::fetch_one` (`onchain.rs:112`) which builds `EthCallRequest::new(contract, calldata)`, calls `router.eth_call`, and decodes. **The `decoder_id` comes from the manifest spec**, not from the wire — it selects the hand-coded `DecoderRegistry` entry (e.g. `lido_wsteth_by_steth`, landed `03de73e3`), falling back to `AbiDecoder` (`onchain.rs:95-110`).
2. **Concrete `eth_call` method (only if a manifest explicitly declares one).** If a manifest author wants a direct view, they must define a concrete `method` (e.g. `chain.eth_call` / `onchain.view`) with an **agreed structured-JSON `params` shape** carrying `{chain_id, contract, function-or-selector, args[]}`, returned under `$.result.*`. **This contract does not exist in code yet** and must be designed alongside its server handler if needed. The dormant `DataSource::OnchainView { chain, contract, function, decoder_id }` (`source.rs:53-64`) is a **different v3-decode channel** (mappers crate, **no `args` field**, `PROTOCOL_ONBOARDING_AND_TESTING.md:610`) — **it does not flow through `dispatchCallsV2`** and must not be conflated.

The `result` JSON shape is **per-method**, decided by the server, and read only via the manifest's `outputs[].from = "$.result.<path>"` selectors. The engine never inspects the shape beyond those selectors.

---

## 4. Server design (fetch + decode ONLY)

> **Scope discipline:** the handler does **fetch + decode** and nothing else. **Planning** (`plan_action_rpc_v2_json`) and **materialization into Cedar** (`materialize_v2`) are **client/WASM-side** and must NOT be re-implemented on the server. The server returns **data, never a verdict**.

### 4.1 Route registration — which router tier

Routes register in `build_router_with_config(state, config) -> Router` (`app.rs:145`). Add to the chain following the existing convention (route → thin `*_handler` wrapper, not the business fn — cf. `/evaluate → evaluate_handler`, `app.rs:148`):

```rust
// app.rs — choose ONE tier:
// PROTECTED tier (app.rs:146-179), behind require_auth — recommended IF a JWT can be issued to the SW:
let protected = Router::new()
    .route("/evaluate", post(evaluate_handler))
    .route("/v1/rpc", post(rpc_handler))   // <-- new
    // ...
    .layer(from_fn(require_auth));          // app.rs:179

// PUBLIC tier (app.rs:181-188), no auth — required IF the SW sends no JWT (today's reality):
let public = Router::new()
    .route("/health", get(health_handler))
    .route("/v1/rpc", post(rpc_handler));  // <-- new
```

**Tension:** the extension SW sends **no auth cookie/JWT**, so `/v1/rpc` would have to be **public**, which **breaks the server's uniform `require_auth` boundary** (`app.rs:179`). This is the central coordination decision (§8). The security posture (§5) treats it as **public-but-guarded** (capability token + rate-limit + whitelist), not silently unauthenticated. `post`/`get` already imported (`app.rs:12`).

### 4.2 Handler signature — two layers (mirror `/evaluate`)

**(1) Route-target wrapper** in `app.rs` (axum extractors; drop `Extension(user)` if public), mirroring `evaluate_handler` (`app.rs:235-259`):

```rust
async fn rpc_handler(
    State(state): State<AppState>,                 // app.rs:236 pattern
    // Extension(user): Extension<AuthUser>,        // ONLY if behind require_auth
    Json(req): Json<PolicyRpcBatchRequest>,         // app.rs:238 pattern
) -> Response {                                      // axum::response::Response
    match rpc(&state, req).await {
        Ok(resp) => Json(resp).into_response(),      // 2xx
        Err(e @ RpcError::Unresolvable(_)) => (StatusCode::OK, Json(e.as_partial())).into_response(), // see note
        Err(e @ RpcError::Provider(_))     => (StatusCode::OK, Json(e.as_partial())).into_response(),
        Err(e @ RpcError::BadRequest(_))   => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
```

> **Error-status nuance (deliberate):** per-call failures must **NOT** surface as a non-2xx HTTP status — a non-2xx makes the SW throw and discard the **whole batch** (`policy-rpc.ts:394`). Per-call failures belong **inside `results[]` as `{ok:false}` entries** (which the SW omits, fail-closing only that required call). Reserve non-2xx for **batch-level** faults (malformed body, oversize, rate-limited 429). This is the inverse of `/evaluate`'s status mapping — call it out in review.

**(2) Business-logic fn** in `handler.rs` (or a new `policy_rpc_handler.rs` module), store-trait/axum-free, mirroring `evaluate` (`handler.rs:80-83`) with a `HandlerError`-style enum (`handler.rs:27-63`):

```rust
pub async fn rpc(state: &AppState, req: PolicyRpcBatchRequest)
    -> Result<PolicyRpcResponse, RpcError>;

pub enum RpcError { BadRequest(String), Unresolvable(String), Provider(String) }
```

It echoes `req.request_id`, builds a **fresh per-request results map** (the same `call_id` repeats across actions — a shared map would clobber, CLAUDE.md spine note), and for each call: whitelist-resolve → `OnchainViewFetcher::fetch_one`/`fetch_batch` → push `{id, ok:true, result}` or `{id, ok:false, error}`.

### 4.3 AppState injection of the fetcher (recommended: no new field)

The `Orchestrator` already **owns** an `OnchainViewFetcher` (`orchestrator.rs:44`) built from the same router and exposes `router_arc() -> Option<Arc<RpcRouter>>` (`orchestrator.rs:80-83`). `AppState` already carries `orchestrator: Arc<Orchestrator>` (`app.rs:47`) with a `FromRef` sub-extractor (`app.rs:102-106`).

**Recommended (zero churn):** the handler pulls `state.orchestrator.router_arc()` and constructs `OnchainViewFetcher::new(router)` per request, **or** add an `Orchestrator` method that delegates to its owned `onchain` fetcher. **No new `AppState` field**, so no edits to the `Debug` impl (`app.rs:63-81`) or to **every** `AppState { ... }` literal (`main.rs:105-115`, plus tests `local_only_policy_verdict_routes.rs:27`, `read_endpoints.rs:47`, `sse.rs:35`, `server_with_postgres.rs:58`).

**Alternative (explicit, more churn):** add `pub fetcher: Arc<OnchainViewFetcher>` to `AppState` (`app.rs:37-61`) + `Debug` field (`app.rs:63-81`) + a `FromRef` impl + **every** construction site, building it in `main.rs` after line 80 as `Arc::new(OnchainViewFetcher::new(orchestrator.router_arc().expect("rpc router")))`. The router only comes from `Orchestrator::from_sync_config`/`from_rpc_router` over the loaded `sync_config` (`main.rs:80`), so it must be derived from that same config. **Prefer the recommended path** unless the explicit field aids testability.

Public exports are all reachable as `policy_sync::OnchainViewFetcher / OnchainCall / OnchainOutcome / RpcRouter / EthCallRequest / Orchestrator` (`sync/src/lib.rs:136-137,197-199,203`).

### 4.4 Config env vars (`config.rs`)

Add fields to `ServerConfig` (`config.rs:9-33`), populate in `from_env()` (`config.rs:38`), and **also** add to `for_tests()` (`config.rs:79-95`, constructs all fields literally or won't compile):

| Field | Pattern | Source |
|---|---|---|
| `policy_rpc_enabled: bool` | `env_bool("POLICY_RPC_ENABLED", false)` — **disabled by default** | helper `config.rs:98-102` |
| `policy_rpc_provider_url: Option<String>` | `env::var("POLICY_RPC_PROVIDER_URL").ok()` — `None` when unset | optional pattern `config.rs:64` |
| `policy_rpc_capability_token: Option<String>` | `env::var("POLICY_RPC_TOKEN").ok()` | optional pattern `config.rs:64` |
| `policy_rpc_max_batch: u64` | `env_u64("POLICY_RPC_MAX_BATCH", 8)` | helper `config.rs:104-109` |
| `policy_rpc_budget_ms: u64` | `env_u64("POLICY_RPC_BUDGET_MS", 6000)` — under SW `HARD_TIMEOUT_MS=8000` | helper `config.rs:104-109` |

> The **RPC provider URL** that the fetcher actually dials comes from the loaded `SyncConfig.rpc` (`RpcConfig`, consumed by `RpcRouter::from_config`, `router.rs:35`) — the router is already built from `sync_config` at `main.rs:80`. `POLICY_RPC_PROVIDER_URL` above is for a **separate enrichment-tier provider/key** (recommended in §5 so abuse can't starve core sync); if reusing the sync provider, this field is the explicit opt-in and the **enable gate** is `POLICY_RPC_ENABLED`. When disabled, register no route (or return 503), preserving today's "verdicts 100% local WASM Cedar" default (`LIMITATIONS.md:172`).

---

## 5. Security threat model

> **Posture:** ship `/v1/rpc` **ONLY in whitelisted-method mode**, never as an arbitrary `eth_call` relay. The single whitelist control (resolve every call to a registry-known spec server-side and reconstruct `to`/`data` from the manifest, refusing raw calldata) simultaneously closes the open-proxy/SSRF surface, neutralises hostile-target gas-bombs, and makes the verdict-isolation property hold in practice. Layer it with batch/body caps, per-IP rate-limit, a `ttl_s`-driven cache, returndata/gas ceilings, `allow_private_network=false`, and a build-baked capability token.

| Threat | Severity | Mitigation | Enforced where |
|---|---|---|---|
| **Open `eth_call` relay / SSRF** — endpoint forwards arbitrary `{to,data,from,value,block}` verbatim. `PublicRpcProvider.eth_call` (`providers/public.rs:97-115`) copies caller `to/data/from/value` with **no allow-list** → free credentialed `eth_call` proxy over every configured chain. (Outbound URL is **not** caller-controlled — router selects by chain, unknown chain errors in `RpcRouter.try_all` — so classic SSRF-to-metadata is bounded, but it is still a credentialed relay.) | **High** | **WHITELIST against the registry:** do **NOT** accept raw `{to,data}`. Accept only `(method, params)`; look the method/`call_id` up in a **server-side mirror of the registry call-specs**; reconstruct `(chain, contract, selector, ABI-typed params, decoder_id)` **from the manifest**; reject anything that doesn't resolve. Pin `block=latest`; refuse caller `from`/`value` (view calls need neither). Collapses the relay to the finite set of registry-vouched enrichment reads. **Arbitrary-call mode must never ship.** | New `/v1/rpc` handler: a registry-spec resolver layer (mirror `registryV2` by-callkey known-spec set) in front of `RpcRouter.eth_call`; belt-and-suspenders egress allow-list of manifest-universe addresses. |
| **Unauthenticated DoS + RPC credit/quota exhaustion** — endpoint is unauthenticated (SW sends no JWT), breaking the `require_auth` invariant (`app.rs:179`). Each call = a real provider request; large `calls` batches drain budget and knock enrichment offline (fail-closed → degraded verdicts). | **High** | (a) Per-request **batch cap** (`policy_rpc_max_batch`, small) + body/Content-Length limit. (b) Per-IP/origin rate-limit. (c) **Build-baked capability token** (the SW already carries `POLICY_RPC_*` config) — turns "fully open" into "open to anyone who unpacked the extension", acceptable with rate-limit + whitelist. (d) Aggressive **caching** (TTL item) so repeats cost zero. (e) Bound outbound concurrency + hard per-batch deadline (`policy_rpc_budget_ms` < SW `HARD_TIMEOUT_MS=8000`; provider client already 15s). | Handler + tower middleware (body-limit + rate-limit) + `RpcRouter` call budget. **Separate enrichment provider key/quota** from the trusted sync worker. |
| **Malicious target on `eth_call`** — gas-bomb (unbounded loop on provider compute), forced revert, or huge return-data blob. `eth_call` has no caller-paid gas; `decode_hex_bytes` (`public.rs:246`) decodes full returndata into `Vec<u8>` with **no size cap**. | **Medium** | Whitelisting (row 1) is primary — arbitrary hostile `to` is unreachable. Defense in depth: cap returndata size (reject oversize decode), inject an explicit `gas` ceiling into the `eth_call` params, per-call wall-clock timeout < 15s, treat revert/oversize as **result-absent** (fail-closes the required call) rather than propagating attacker bytes into context. | Handler (returndata guard + gas ceiling in `EthCallRequest`) + `PublicRpcProvider` bounded decode. |
| **Origin / CORS** — only the SW should reach this, but CORS is browser-enforced and does **not** stop curl/server-to-server. Current allowlist (`config.rs:47-61`) holds only dashboard dev origins; `allow_private_network` defaults **TRUE** (`config.rs:62`). SW `Origin` is `chrome-extension://<id>`/`null`, not in the list. | **Medium** | Do **NOT** rely on CORS as authN/authZ. Add the published extension origins (`chrome-extension://<stable-id>`, `moz-extension://<id>`) to the allowlist for preflight UX, but enforce real access via **token + rate-limit + whitelist**. Set `allow_private_network=false` in cloud. **Never `AllowOrigin::any()`.** Treat non-whitelisted origins as an abuse signal, not the security boundary. | `app.rs` `cors_layer` for preflight ergonomics; binding control is handler token check + middleware. |
| **No response caching / TTL** — every pre-sign action re-fetches the same view (decimals, `is_contract`, `slot0`), multiplying provider cost, DoS amplification, and signing latency. Manifests already carry `ttl_s` (e.g. `"ttl_s": 12`). | **Medium** | Cache `eth_call` results keyed by `(chain, to, data)` (or by `call_id` once whitelisted) with **TTL = the spec's `ttl_s`** (sane floor 5–15s; cap the max so stale enrichment can't persist). Serve hits with zero provider calls; a flood of identical calls collapses to one upstream call per TTL window. **Must not mix block tags.** | Handler in-process LRU/TTL cache (or Redis when `REDIS_URL` set — `AppState` already carries a coordinator/Redis boundary). TTL from the registry spec `ttl_s`. |
| **No per-origin/IP rate-limit** — without it the rows above are unbounded; one unauthenticated client issues unlimited batches. | **Medium** | Token-bucket per source IP (and per capability token), low steady-state + small burst sized to human signing cadence; **429** on exceed. Global concurrency/QPS cap toward the provider. Per-IP is the practical key (endpoint unauthenticated); pair with cache. | tower-governor / custom middleware on the `/v1/rpc` route; global provider-facing semaphore in `RpcRouter`. |
| **Verdict-isolation is NOT structural** — a policy authored to **gate** on an enrichment field will let a wrong enrichment value flip the verdict. Proven by the engine's own E2E test (`materialize_v2.rs:449-526`): a forbid policy `when { context.custom.totalInputUsd.greaterThan(decimal("1000.0000")) }` evaluates to **Warn** purely because the enriched value was `3500`. `materialize_v2` writes results into `context.custom.*` (`materialize_v2.rs:97,123`) and Cedar may freely read them. A hostile `/v1/rpc` returning an attacker-chosen number → attacker-chosen verdict **for gating policies**. | **High** | State the property precisely: **isolation holds ONLY for policies that treat `context.custom.*` as display-only and never branch on it.** Mitigations: (1) the **whitelist** (row 1) is what protects value integrity — a registry-defined view on a known contract via the server's own provider is hard for a dApp/web attacker to forge (no caller `to`/`data`). (2) Keep the shipped default catalog **free of gating on enrichment fields** (display-only); add a **registryV2 lint** flagging any policy whose condition references `context.custom.<enrichment-field>` so authors opt in knowingly. (3) Dispatch fails **closed** — a *down/blocked* enrichment can only make a gating policy **more** conservative; the dangerous case is a *wrong-but-present* value, which the whitelist addresses. | Integrity at policy-server whitelist + local WASM Cedar (verdict computed locally; server returns data only). Author-time guard: registryV2 lint + a documented warning in the policy authoring guide. |

**Fail-closed note (cuts two ways):** for **absence** of an enrichment value the system is genuinely fail-closed — `dispatchCallsV2` omits unreachable/errored calls (`policy-rpc.ts:351-378`) and `materialize_v2` raises `SystemFail` for a missing/failed **required** call (`materialize_v2.rs:52-61`) → `__system__` fail-closed verdict (venue/HL orders deny-closed). So an attacker who **blocks/downs** the endpoint **cannot wave an action through** — at worst a forced warn/deny. **Residual exposure = a wrong-but-present value:** a well-formed malicious result is **not** caught by fail-closed (the call succeeded); it matters **only if a policy gates** on that `context.custom` field (`materialize_v2.rs:449-526`). For display-only policies a wrong value mis-renders the USD/converted figure shown to the user but **cannot change the Cedar verdict** (verdict is local WASM from calldata; enrichment populates display fields). The whitelist (no caller-chosen `to`/`data`) is what keeps gating-consumed values trustworthy.

---

## 6. Implementation steps (ordered; blockers noted)

> Legend: **[FW]** framework-wide (new work, this design) · **[DONE]** already landed · **[COORD]** needs woojinnn.

1. **[DONE]** Lido decoder slice in `decoder.rs` (`lido_wsteth_by_steth` etc.), commit `03de73e3`. Decode-by-id is ready; this design does **not** add Lido decoders.
2. **[FW][COORD]** **Decide router tier + auth** (§4.1, §8): public-but-guarded vs protected-with-SW-JWT. **Blocks** route registration and the capability-token design.
3. **[FW]** **Config** (§4.4): add `policy_rpc_enabled` (default false) + provider/token/cap/budget fields to `ServerConfig` (`config.rs:9-33`), `from_env()` (`config.rs:38`), **and** `for_tests()` (`config.rs:79-95`). **Blocks** route gating + handler. *(no downstream code depends on the others first)*
4. **[FW]** **Registry call-spec resolver / whitelist** (§5 row 1) — server-side mirror of registry specs keyed by `call_id`/`method`, reconstructing `(chain, contract, selector, decoder_id, args)`. **This is the load-bearing control; it blocks any safe handler.** **Blocks** step 6.
5. **[FW]** **Fetcher access** (§4.3): expose `OnchainViewFetcher` via `state.orchestrator.router_arc()` (recommended) or add an orchestrator delegate method. *(independent of step 4; can land in parallel)*
6. **[FW]** **Business handler `rpc(...)`** in `handler.rs` (§4.2 layer 2): per-request fresh map, whitelist-resolve (step 4) → `fetch_one`/`fetch_batch` (`onchain.rs:112,118`) → `{id,ok,result|error}`, echo `request_id`. **Depends on 3,4,5.**
7. **[FW]** **Route wrapper `rpc_handler`** + registration in `app.rs` (§4.1, §4.2 layer 1), gated on `policy_rpc_enabled`. **Depends on 2,6.**
8. **[FW]** **Middleware**: body/batch cap, per-IP rate-limit (429), CORS extension-origin allowlist, `allow_private_network=false` (§5 rows 2,4,6). *(layered onto step 7's route)*
9. **[FW]** **Cache** keyed by `(chain,to,data)`/`call_id`, TTL = spec `ttl_s` (§5 row 5). *(layered; can follow step 7)*
10. **[FW]** **Hardening**: returndata-size guard + `gas` ceiling in `EthCallRequest` (§5 row 3); optional bounded decode in `PublicRpcProvider`.
11. **[FW]** **registryV2 lint**: flag policies gating on `context.custom.<enrichment-field>` (§5 row 7). *(parallel; not on the server critical path)*
12. **[FW]** **Tests** (§7) interleaved with 6–10 (TDD).

---

## 7. Test plan

**Unit (handler, no network):**
- Resolver: a known `call_id`/`method` reconstructs the expected `(chain, contract, selector, decoder_id)` from the manifest mirror; an unknown method → `{ok:false}`/omit (never an `eth_call`).
- Handler builds the **correct** `EthCallRequest` (`onchain.rs:112` path): contract + ABI-typed calldata derived from structured `params`; `block=latest`; no caller `from`/`value`.
- Decode-by-id: a canned returndata for the Lido spec decodes via `DecoderRegistry` (`lido_wsteth_by_steth`, `03de73e3`) to the expected `{ "...": ... }`, and an unknown id falls back to `AbiDecoder` (`onchain.rs:95-110`).
- Response shaping: `request_id` echoed verbatim; per-call success `{id,ok:true,result:<unwrapped>}`; failure `{id,ok:false,error}`; fresh per-request map (same `call_id` across two batches does not clobber).

**Integration (mock RPC provider):**
- Stub `RpcRouter`/provider; assert one upstream `eth_call` per uncached call, cache hit serves zero upstream calls within `ttl_s`, batch cap rejects oversize (non-2xx, batch-level), rate-limit returns 429.
- Gas-bomb/oversize returndata → result-absent `{ok:false}` (not propagated bytes); revert → `{ok:false}`.

**E2E (extension → server → eth_call):**
- Build extension with `POLICY_RPC_*` pointed at a local server + mock/forked RPC. Drive a Lido action whose manifest plans an enrichment call; assert the SW POSTs `/v1/rpc` with the resolved batch, the server fetches+decodes, the SW folds `map[call_id]=result`, and `materialize_v2` writes `context.custom.*` so a **display-only** policy renders the value with **no verdict change**.

**Fail-closed / unreachable / bad-result:**
- Server down / non-2xx → SW throws, omits the call, **required** call → `__system__` fail-closed verdict (`materialize_v2.rs:52-62`); confirm a tx/typed-sig warn-closes and an HL order deny-closes.
- Server returns `ok:false` for a required call → same fail-closed verdict (never waved through).
- **Wrong-but-present** value into a **gating** policy → reproduce `materialize_v2.rs:449-526` to assert the value *does* flip the verdict (documents the residual risk), and assert the whitelist rejects an attacker-supplied raw `{to,data}` so the value cannot be attacker-chosen in practice.
- Malformed response (missing `request_id` echo / `results` not an array) → SW rejects whole batch (`policy-rpc.ts:398`).

---

## 8. Open questions / coordination (woojinnn)

1. **Auth boundary (the central decision).** The SW sends **no JWT**, yet the server's invariant is `require_auth` on every non-public route (`app.rs:179`). Options: (a) **public route + build-baked capability token + rate-limit + whitelist** (pragmatic, recommended); (b) mint a **scoped SW JWT** and keep it protected (cleaner, needs token-issuance plumbing to the extension). **woojinnn decides** since it touches the auth layer he owns.
2. **Whitelist strictness & source of truth.** Server-side mirror of `registryV2` call-specs — generated/synced how (vendored snapshot vs fetched from `registry-api`)? How keyed (`call_id` vs `method`)? Belt-and-suspenders egress address allow-list — derive from the manifest universe at build time?
3. **Provider choice & isolation.** Reuse the sync `SyncConfig.rpc` provider (`router.rs:35`, `main.rs:80`) or a **separate enrichment-tier key/quota** so abuse can't starve core sync (recommended)? Public vs paid tier sets the rate-limit/budget defaults.
4. **Cache backend.** In-process LRU/TTL (simplest) vs Redis when `REDIS_URL` set (shared across instances; `AppState` already has a coordinator/Redis boundary). TTL floor/ceiling around the spec `ttl_s`.
5. **Concrete `eth_call` method?** Do we need a generic `chain.eth_call`/`onchain.view` method (§3.3 mechanism 2), or is registry-keyed resolution (mechanism 1) sufficient for all near-term manifests (Lido included)? If yes, **defer** the generic method — it adds attack surface.
6. **Enable-gate default & rollout.** `POLICY_RPC_ENABLED=false` by default (preserves today's 100%-local verdicts, `LIMITATIONS.md:172`); flip per-environment once whitelist + rate-limit + cache land.

**woojinnn coordination point:** he owns the axum `policy-server` — the **route tier + auth decision (Q1)**, the **middleware stack** (rate-limit/body-limit/CORS), the **config additions** (`config.rs`), and the **provider/quota isolation (Q3)** are his surface. The ScopeBall maintainer owns the **registry call-spec mirror / whitelist (Q2)**, the **`ttl_s`-driven cache contract (Q4)**, and the **registryV2 gating-lint** (§5 row 7). The wire contract (§3) is **fixed by the already-shipped client** — neither side may change `request_id`-echo, the `{id,ok,result}` entry shape, or the unwrapped-`$.result` payload convention without a coordinated extension change.

---

## 9. Scope boundary

| In scope (this design) | Out of scope |
|---|---|
| **[FW]** New `POST /v1/rpc` **fetch+decode-only** handler on the policy-server (route, wrapper, business fn). | Re-implementing **planning** (`plan_action_rpc_v2_json`) or **materialization** (`materialize_v2`) on the server — both stay client/WASM-side. |
| **[FW]** Registry call-spec **whitelist/resolver**, fetcher injection via `Orchestrator`, config env vars + enable gate. | The server **never** returns a verdict — verdicts remain 100% local WASM Cedar. |
| **[FW]** Security middleware (rate-limit, body/batch cap, cache, gas/returndata ceilings, CORS, `allow_private_network=false`), registryV2 gating-lint. | New domains, new `policy_rpc[]` manifest specs beyond what already exists, or the dormant v3 `DataSource::OnchainView` channel (`source.rs:53-64`) — a **different** mappers-crate path, not this wire. |
| **[FW]** Tests (unit/integration/e2e/fail-closed). | Issuing per-manifest enrichment specs for protocols other than the Lido slice. |
| **[DONE]** The **Lido decoder slice** (`lido_wsteth_by_steth` etc.) — already landed in `decoder.rs`, commit `03de73e3`. This design **consumes** it via `decoder_id`; it adds **no** Lido-specific decoder code. The gap is **framework-wide** (the missing hop-6 server route), not Lido-specific. |

> The Lido slice proves decode-by-id works for one protocol; this design supplies the **framework hop-6** that every protocol's enrichment needs. Once `/v1/rpc` ships in whitelisted mode and `POLICY_RPC_ENABLED=true` with a provider configured, enrichment goes live for **all** registry call-specs, not just Lido.

---

## 10. Quick reference — key `file:line`

- **The gap:** `crates/policy-server/server/src/app.rs:146-188` (no `/v1/rpc` route in either router tier)
- **Route registration site:** `crates/policy-server/server/src/app.rs:145` (`build_router_with_config`)
- **Handler pattern to mirror:** `crates/policy-server/server/src/app.rs:235-259` (`evaluate_handler`) + `handler.rs:80-83` (`evaluate`)
- **Fetcher to reuse:** `crates/policy-server/sync/src/sources/fetchers/onchain.rs:112,118` via `state.orchestrator.router_arc()` (`orchestrator.rs:80-83`)
- **Config site:** `crates/policy-server/server/src/config.rs:9-33,38,79-95`
- **Wire contract (fixed by shipped client):** `browser-extension/backend/service-worker/policy-rpc.ts:382-421` + `wasm-bridge.types.ts:12-26`
- **Load-bearing security control:** whitelist `(method, params)` against registry call-specs — never an arbitrary `eth_call` relay (`PublicRpcProvider.eth_call`, `providers/public.rs:97-115`)
- **Verdict-isolation residual proof:** `crates/policy-engine/src/policy_rpc/materialize_v2.rs:449-526`
- **Already-done:** Lido decoder slice, commit `03de73e3` (`decoder.rs`)
