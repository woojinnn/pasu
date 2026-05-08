# WASM Bridge Crate — Plan 2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compile the policy engine to WebAssembly with a JSON-string boundary the Chrome extension's TS code can consume — five exports: `install_policies_json`, `build_action_json`, `tier1_fact_plan_json`, `tier2_window_keys_json`, `evaluate_json`.

**Architecture:** New crate `crates/policy_engine_wasm/` targeting `wasm32-unknown-unknown` via `wasm-pack`. Holds a thread-local `PolicyEngine` after `install_policies_json` populates it. Per-call exports build pipelines on-the-fly from a `MockAdapterRegistry` populated by `default_registry()`, a `SnapshotOracle` from the host snapshot JSON, and a fresh request from the request JSON. JSON-in / JSON-out is the only boundary.

**Tech Stack:** `wasm-bindgen`, `serde-json`, `wasm-pack` build tooling. New `cedar-policy/wasm` feature (already shipped in cedar-policy 4.x). No new heavyweight deps.

**Series:** Plan 2 of the Chrome-extension series. Depends on Plan 1's engine API additions (`SnapshotOracle`, `Pipeline::build_action_for`, `required_host_facts`, `required_window_keys`).

**Scope (in this plan):** Pure Rust+WASM. Produces an artifact `pkg/policy_engine_wasm_bg.wasm` + JS glue, ready for the extension to import. No TS code, no extension files.

**Out of scope:** Browser-side loading, IndexedDB caching of compiled module, any extension UX or RPC code.

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `crates/policy_engine_wasm/Cargo.toml` | Create | Crate manifest, wasm32 features, lib type cdylib |
| `crates/policy_engine_wasm/src/lib.rs` | Create | Module wiring + WASM init |
| `crates/policy_engine_wasm/src/state.rs` | Create | Thread-local `EngineState { policies: PolicyEngine, registry: MockAdapterRegistry, signature_registry: MockSignatureRegistry }` |
| `crates/policy_engine_wasm/src/dto.rs` | Create | `VerdictDto`, `MatchedPolicyDto`, `HostFactPlanDto`, `WindowKeyPlanDto`, `HostSnapshotDto` (serde-friendly mirrors of engine types) |
| `crates/policy_engine_wasm/src/exports.rs` | Create | Five `#[wasm_bindgen]` functions |
| `crates/policy_engine_wasm/tests/web.rs` | Create | wasm-bindgen-test cases for each export |
| `Cargo.toml` (workspace root) | Modify | Add `crates/policy_engine_wasm` to `[workspace.members]`, add `wasm-bindgen.workspace` entry |
| `.github/workflows/wasm.yml` | Create | CI job: build wasm via wasm-pack, run wasm tests |
| `crates/policy-engine/Cargo.toml` | Modify | Optional: gate `chrono`/`std::time` calls behind a feature flag if any are not wasm-safe (verify in Task 1) |

---

## Task 1: Verify wasm32 build viability for policy-engine

**Files:** None modified — investigation only. Outcome dictates whether feature flags are needed.

- [ ] **Step 0: Tier-2 WindowKey serialization to wire strings**

The `tier2_window_keys_json` export emits `WindowKeyDto { actor, name }` where `name` is the canonical wire string (`StatKey::as_str()` — i.e. `"swapVolumeUsd24h"` / `"swapCount24h"`). Add an explicit conversion in `dto.rs`:

```rust
impl From<&policy_engine::lowering::WindowKey> for WindowKeyDto {
    fn from(k: &policy_engine::lowering::WindowKey) -> Self {
        Self {
            actor: k.actor.as_str().to_lowercase(),
            name: k.key.as_str().to_string(), // StatKey::as_str() is the wire string
        }
    }
}
```

This ensures the TS side reads stable `swapVolumeUsd24h` strings and the round-trip back through `evaluate_json`'s `windows[].name` matches. **Never use plan-level snake_case strings** — those don't exist in the engine.

- [ ] **Step 1: Add the wasm target**

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version "^0.13" --locked
cargo install wasm-bindgen-cli --version "^0.2.93" --locked
```

Expected: each install reports success. `wasm-pack --version` returns 0.13.x.

- [ ] **Step 2: Try a dry-run wasm build of the engine**

```bash
cargo build -p policy-engine --target wasm32-unknown-unknown 2>&1 | tail -40
```

If it succeeds: skip Step 3 and Step 4.

If it fails on `std::time::SystemTime`, `mio`, `tokio`, or similar non-wasm symbols: capture the failing crate names. Most common offender for cedar-policy is `getrandom` without the `js` feature.

- [ ] **Step 3: If `getrandom` fails, add the `js` feature opt-in**

Edit workspace `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
getrandom = { version = "0.2", features = ["js"] }
```

Edit `crates/policy-engine/Cargo.toml`, add a target-specific dep section:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
getrandom.workspace = true
```

Re-run Step 2. Iterate on missing-feature errors until the build succeeds.

- [ ] **Step 4: If `SystemClock` references `std::time::SystemTime` and that breaks wasm, gate it**

Search:

```bash
grep -n "SystemTime\|std::time" crates/policy-engine/src/host/clock.rs
```

If `SystemTime::now()` is called unconditionally and breaks wasm, refactor `SystemClock::now_unix` to `#[cfg(not(target_arch = "wasm32"))]`. The wasm path uses `js_sys::Date::now() / 1000.0` instead. Propose this change as its own commit:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn now_unix(&self) -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn now_unix(&self) -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}
```

Add `js-sys = "0.3"` and `wasm-bindgen = "0.2"` to a `[target.'cfg(target_arch = "wasm32")'.dependencies]` block in the policy-engine crate.

- [ ] **Step 5: Document the verified build command**

If Steps 2–4 produced clean wasm output, capture the exact command in `crates/policy_engine_wasm/Cargo.toml` rustdoc. (Cosmetic — but locks in the build invocation for future debugging.)

- [ ] **Step 6: Commit any policy-engine changes from Steps 3–4**

If files outside `crates/policy_engine_wasm/` were modified:

```bash
git add crates/policy-engine/Cargo.toml Cargo.toml
git commit -m "$(cat <<'EOF'
feat(engine): wasm32 build feature gating

Adds target-cfg branches so policy-engine compiles cleanly to
wasm32-unknown-unknown. SystemClock::now_unix uses js_sys::Date on
wasm; std::time on native. getrandom gains the "js" feature on wasm.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Scaffold the wasm crate

**Files:**
- Create: `crates/policy_engine_wasm/Cargo.toml`
- Create: `crates/policy_engine_wasm/src/lib.rs`
- Modify: workspace `Cargo.toml`

- [ ] **Step 1: Add the crate to the workspace**

Edit workspace `Cargo.toml`. Find `[workspace.members]` and append `"crates/policy_engine_wasm"` to the list. Add to `[workspace.dependencies]`:

```toml
wasm-bindgen = "0.2"
js-sys = "0.3"
serde-wasm-bindgen = "0.6"
console_error_panic_hook = "0.1"
wasm-bindgen-test = "0.3"
```

- [ ] **Step 2: Create `crates/policy_engine_wasm/Cargo.toml`**

```toml
[package]
name = "policy-engine-wasm"
version.workspace = true
edition.workspace = true
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
policy-engine.workspace = true
policy-engine-adapters-bundle.workspace = true
serde.workspace = true
serde_json.workspace = true
wasm-bindgen.workspace = true
js-sys.workspace = true
console_error_panic_hook.workspace = true
thiserror.workspace = true

[dev-dependencies]
wasm-bindgen-test.workspace = true

# NOTE: workspace root has no [lints] table; do not add `lints.workspace = true`.
```

- [ ] **Step 3: Create the lib.rs entry point**

```rust
//! WASM bridge for the policy engine.
//!
//! Five JSON-in/JSON-out exports surface the engine to TypeScript:
//! `install_policies_json`, `build_action_json`, `tier1_fact_plan_json`,
//! `tier2_window_keys_json`, `evaluate_json`. The boundary is JSON strings
//! so the Verdict and PolicyRequest types do not need to derive Serialize.

mod dto;
mod exports;
mod state;

use wasm_bindgen::prelude::*;

/// Module init: forward Rust panics to the JS console.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

pub use exports::{
    build_action_json, evaluate_json, install_policies_json, tier1_fact_plan_json,
    tier2_window_keys_json,
};
```

- [ ] **Step 4: Create empty stubs to satisfy compilation**

Create `crates/policy_engine_wasm/src/state.rs`:

```rust
//! Per-WASM-instance state: holds the installed PolicyEngine + adapter registries.
//!
//! `chrome.runtime` instantiates one WASM module per service worker; the SW
//! may be killed and re-instantiated, after which `install_policies_json` is
//! called again. State is a thread-local `RefCell<Option<EngineState>>`.

use policy_engine::policy::PolicyEngine;
use policy_engine_adapters_bundle::{default_registry, default_signature_registry};
use std::cell::RefCell;

pub struct EngineState {
    pub policies: PolicyEngine,
}

thread_local! {
    pub static STATE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

#[must_use]
pub fn registry() -> impl policy_engine::registry::AdapterRegistry {
    default_registry()
}

#[must_use]
pub fn signature_registry() -> impl policy_engine::registry::SignatureRegistry {
    default_signature_registry()
}
```

Create `crates/policy_engine_wasm/src/dto.rs`:

```rust
//! Serde-friendly DTOs that mirror engine types at the JSON boundary.
//!
//! The engine's internal Verdict / MatchedPolicy / PolicyRequest do not all
//! derive Serialize; rather than mutate engine internals we re-shape into
//! these DTOs at the wasm boundary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerdictDto {
    Pass,
    Warn { matched: Vec<MatchedPolicyDto> },
    Fail { matched: Vec<MatchedPolicyDto> },
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchedPolicyDto {
    pub policy_id: String,
    pub reason: Option<String>,
    pub severity: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostFactPlanDto {
    pub tokens_for_oracle: Vec<TokenDto>,
    pub balances: Vec<BalanceFactDto>,
    pub allowances: Vec<AllowanceFactDto>,
    pub clock_required: bool,
    pub sig_oracle_requirements: Vec<OracleRequirementDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowKeyPlanDto {
    pub keys: Vec<WindowKeyDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowKeyDto {
    pub actor: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenDto {
    pub chain_id: u64,
    pub address: String,
    pub symbol: String,
    pub decimals: u32,
    pub is_native: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceFactDto {
    pub owner: String,
    pub token: TokenDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowanceFactDto {
    pub owner: String,
    pub token: TokenDto,
    pub spender: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OracleRequirementDto {
    pub kind: String,
    pub token: TokenDto,
    pub raw_amount: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostSnapshotDto {
    pub oracle: Vec<OracleEntryDto>,
    #[serde(default)]
    pub balances: Vec<BalanceEntryDto>,
    #[serde(default)]
    pub allowances: Vec<AllowanceEntryDto>,
    /// Unix seconds. Engine `Clock::now() -> u64`, so we surface as u64 here too.
    #[serde(default)]
    pub now_ts: Option<u64>,
    #[serde(default)]
    pub windows: Vec<WindowEntryDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OracleEntryDto {
    pub token_key: String,
    pub usd_per_unit: String,
    pub as_of_ts: u64,
    #[serde(default)]
    pub stale_sec: u64,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BalanceEntryDto {
    pub owner: String,
    pub token_key: String,
    pub balance: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllowanceEntryDto {
    pub owner: String,
    pub token_key: String,
    pub spender: String,
    pub allowance: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WindowEntryDto {
    pub actor: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineErrorDto {
    pub kind: String,
    pub message: String,
}
```

Create `crates/policy_engine_wasm/src/exports.rs`:

```rust
//! Thin `#[wasm_bindgen]` exports. Each function takes a JSON string and
//! returns a JSON string. Errors surface as `Verdict::Fail` JSON with a
//! reason code, never as JS exceptions, so the orchestrator's fail-closed
//! path is uniform.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn install_policies_json(_policies_json: String) -> String {
    todo!("Task 3")
}

#[wasm_bindgen]
pub fn build_action_json(_request_json: String) -> String {
    todo!("Task 4")
}

#[wasm_bindgen]
pub fn tier1_fact_plan_json(_action_json: String) -> String {
    todo!("Task 5")
}

#[wasm_bindgen]
pub fn tier2_window_keys_json(_action_json: String, _oracle_snapshot_json: String) -> String {
    todo!("Task 6")
}

#[wasm_bindgen]
pub fn evaluate_json(_request_json: String, _host_snapshot_json: String) -> String {
    todo!("Task 7")
}
```

- [ ] **Step 5: Verify the crate compiles**

```bash
cargo build -p policy-engine-wasm 2>&1 | tail -10
```

Expected: builds with warnings about `todo!()`. No errors.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/policy_engine_wasm/
git commit -m "$(cat <<'EOF'
feat(wasm): scaffold policy-engine-wasm crate

cdylib + rlib targets, wasm-bindgen entry, DTO module, state holder,
empty exports stubs. Bodies filled by tasks 3-7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `install_policies_json` export

**Files:** Modify `crates/policy_engine_wasm/src/exports.rs`, `state.rs`.

Input shape:

```json
{
  "schema_text": "<concatenated cedar schema>",
  "policy_set": [
    { "id": "<bundle_id>::<policy_name>", "text": "permit ( ... );" },
    ...
  ]
}
```

Returns `{"ok": true}` on success or `{"ok": false, "error": "..."}` on parse/compile error.

- [ ] **Step 1: Define input + output DTOs**

Append to `dto.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct InstallPoliciesInputDto {
    pub schema_text: String,
    pub policy_set: Vec<PolicyEntryDto>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyEntryDto {
    pub id: String,
    pub text: String,
}

/// Unified envelope. EVERY wasm export returns this shape (or its Err
/// variant). TS-side `unwrap()` reads `ok` once and either yields `data`
/// or throws. Replaces the per-export ad-hoc shapes that earlier drafts had.
#[derive(Debug, Serialize)]
#[serde(tag = "ok")]
pub enum Envelope<T: Serialize> {
    #[serde(rename = "true")]
    Ok { data: T },
    #[serde(rename = "false")]
    Err { error: EngineErrorDto },
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self { Self::Ok { data } }
    pub fn err(kind: &str, message: &str) -> Self {
        Self::Err { error: EngineErrorDto { kind: kind.into(), message: message.into() } }
    }
    /// Render to JSON. Infallible because all internal types derive Serialize.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Envelope serializes")
    }
}
```

- [ ] **Step 2: Implement install_policies_json**

Replace the `install_policies_json` body in `exports.rs`:

```rust
use crate::dto::{Envelope, InstallPoliciesInputDto};
use crate::state::{EngineState, STATE};
use policy_engine::policy::PolicyEngineBuilder;

#[wasm_bindgen]
pub fn install_policies_json(policies_json: String) -> String {
    let result = (|| -> Result<(), String> {
        let input: InstallPoliciesInputDto = serde_json::from_str(&policies_json)
            .map_err(|e| format!("invalid input json: {e}"))?;
        // NOTE: PolicyEngineBuilder API is `new() -> Self`,
        // `add_text(src: impl Into<String>) -> Self` (not `add_policy`!),
        // `add_schema_text(src) -> Self`, and `build() -> Result<...>`.
        // Per-policy IDs are encoded as `@id("...")` annotations in the
        // policy text itself. The bundle templater (Plan 6) is responsible
        // for prepending `@id("<bundle_id>::<name>")` so installed policies
        // surface with stable, namespaced ids.
        let mut builder = PolicyEngineBuilder::new().add_schema_text(input.schema_text);
        for p in input.policy_set {
            // Allow callers to omit @id annotations by inserting one if
            // missing. Robust check: scan past leading whitespace AND past
            // line/block comments AND past any non-id annotations to find
            // whether `@id(` appears anywhere in the policy's annotation
            // prefix (before `permit`/`forbid`).
            let text = if has_id_annotation(&p.text) {
                p.text.clone()
            } else {
                format!("@id(\"{}\")\n{}", p.id, p.text)
            };
            builder = builder.add_text(text);
        }
        let policies = builder.build().map_err(|e| format!("build: {e}"))?;

        STATE.with(|s| {
            *s.borrow_mut() = Some(EngineState { policies });
        });
        Ok(())
    })();

    match result {
        Ok(()) => Envelope::<()>::ok(()).to_json(),
        Err(message) => Envelope::<()>::err("install_failed", &message).to_json(),
    }
}

/// Robust @id-annotation detector: returns true when the policy text already
/// contains an `@id(...)` annotation in its annotation prefix (before the
/// first `permit` or `forbid` keyword). Skips // line comments and /* block */
/// comments and other annotations like `@severity(...)`.
fn has_id_annotation(text: &str) -> bool {
    // Strip comments and look for `@id(` before the first `permit`/`forbid`.
    let stripped = strip_cedar_comments(text);
    if let Some(head_end) = stripped.find("permit").or_else(|| stripped.find("forbid")) {
        stripped[..head_end].contains("@id(")
    } else {
        // No head keyword — defensively assume @id present so we don't double-inject.
        stripped.contains("@id(")
    }
}

fn strip_cedar_comments(s: &str) -> String {
    // Cheap one-pass strip. Cedar comments are //-line and /* block */.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
            i = (i + 2).min(bytes.len());
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}
```

> Confirmed against `crates/policy-engine/src/policy.rs`:
> - `PolicyEngineBuilder::new()` (`policy.rs:496`)
> - `PolicyEngineBuilder::add_text(src: impl Into<String>) -> Self` (`policy.rs:514`)
> - `PolicyEngineBuilder::add_schema_text(src) -> Self` (`policy.rs:527`)
> - `PolicyEngineBuilder::build() -> Result<PolicyEngine, PolicyError>` (`policy.rs:538`)
>
> There is **no** `add_policy(id, text)` method — IDs come from `@id("...")` annotations in the policy text itself. The defensive injection above prepends one when missing.

- [ ] **Step 3: Native unit test**

Append to `exports.rs`:

```rust
#[cfg(test)]
mod install_tests {
    use super::*;

    fn minimal_schema() -> String {
        // Use the same schema the engine bundles via PolicySchemaComposer.
        std::fs::read_to_string("../../policy-schema/core.cedarschema")
            .unwrap_or_else(|_| "// fallback empty schema".into())
    }

    #[test]
    fn install_with_no_policies_succeeds() {
        let input = serde_json::json!({
            "schema_text": minimal_schema(),
            "policy_set": []
        })
        .to_string();
        let result = install_policies_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["ok"], "true");
    }

    #[test]
    fn install_with_invalid_schema_returns_err() {
        let input = serde_json::json!({
            "schema_text": "this is not cedar schema",
            "policy_set": []
        })
        .to_string();
        let result = install_policies_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["ok"], "false");
        assert!(parsed["error"].as_str().unwrap().contains("schema"));
    }
}
```

> If `PolicySchemaComposer` already loads the schema, the test can call into it directly instead of reading the file. Adjust to match the actual engine API.

- [ ] **Step 4: Run tests**

```bash
cargo test -p policy-engine-wasm install_tests 2>&1 | tail -10
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/policy_engine_wasm/src/
git commit -m "$(cat <<'EOF'
feat(wasm): install_policies_json export

Parses {schema_text, policy_set} and stows the built PolicyEngine in
thread-local state. Returns {ok:true} or {ok:false,error:...}; never
throws.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `build_action_json` export

**Files:** Modify `crates/policy_engine_wasm/src/exports.rs`, `dto.rs`.

Input: serialized `Request` (engine's existing `serde::Serialize`+`Deserialize`).
Output: serialized `Action` JSON. On error: `{"error": {...}}` JSON.

Since `Request` and `Action` already derive Serde, the boundary is direct.

- [ ] **Step 1: Define wrapper DTOs**

Append to `dto.rs`:

```rust
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum BuildActionResultDto {
    Action(serde_json::Value),
    Err(EngineErrorDto),
}
```

- [ ] **Step 2: Implement**

Replace `build_action_json` body in `exports.rs`:

```rust
use crate::dto::{EngineErrorDto, Envelope};
use crate::state::{registry, signature_registry, STATE};
use policy_engine::core::Request;
use policy_engine::host::{oracle::SnapshotOracle, HostCapabilities};
use policy_engine::Pipeline;

fn empty_pipeline_for_build() -> Result<(), String> {
    STATE.with(|s| {
        if s.borrow().is_none() {
            return Err("policies not installed".into());
        }
        Ok(())
    })
}

#[wasm_bindgen]
pub fn build_action_json(request_json: String) -> String {
    let result = (|| -> Result<serde_json::Value, EngineErrorDto> {
        empty_pipeline_for_build().map_err(|m| EngineErrorDto {
            kind: "not_installed".into(),
            message: m,
        })?;
        let request: Request = serde_json::from_str(&request_json).map_err(|e| EngineErrorDto {
            kind: "invalid_request_json".into(),
            message: e.to_string(),
        })?;

        // build_action_for needs a HostCapabilities + a PolicyEngine; we satisfy
        // both with stubs because no enrichment or evaluation runs in this call.
        let oracle = SnapshotOracle::new();
        let registry = registry();
        let sig_registry = signature_registry();
        let action_result = STATE.with(|s| {
            let state = s.borrow();
            let state = state.as_ref().expect("checked above");
            let host = HostCapabilities::new(&oracle);
            let pipeline = Pipeline::new(&registry, host, &state.policies)
                .with_signature_registry(&sig_registry);
            pipeline
                .build_action_for(&request)
                .map_err(|e| EngineErrorDto {
                    kind: pipeline_error_kind(&e).into(),
                    message: e.to_string(),
                })
        });
        let action = action_result?;
        serde_json::to_value(&action).map_err(|e| EngineErrorDto {
            kind: "serialize_action".into(),
            message: e.to_string(),
        })
    })();

    match result {
        Ok(action_json) => Envelope::ok(action_json).to_json(),
        Err(e) => Envelope::<serde_json::Value>::Err { error: e }.to_json(),
    }
}

fn pipeline_error_kind(err: &policy_engine::pipeline::PipelineError) -> &'static str {
    use policy_engine::pipeline::PipelineError;
    match err {
        PipelineError::Ambiguous(_) => "adapter_ambiguous",
        PipelineError::AdapterBuild(_) => "adapter_build",
        PipelineError::Lowering(_) => "lowering_rejected",
        PipelineError::Policy(_) => "policy",
    }
}
```

- [ ] **Step 3: Native unit test**

Append to `exports.rs`:

```rust
#[cfg(test)]
mod build_action_tests {
    use super::install_tests::minimal_schema;
    use super::*;

    fn install_empty_policies() {
        install_policies_json(
            serde_json::json!({
                "schema_text": minimal_schema(),
                "policy_set": []
            })
            .to_string(),
        );
    }

    #[test]
    fn build_action_returns_other_for_unknown_calldata() {
        install_empty_policies();
        // CONFIRMED FROM SOURCE (core.rs:471):
        // - `Request` has no #[serde] attrs → externally tagged: outer key "Tx".
        // - `TransactionRequest` has no rename_all → snake_case fields.
        // - `data: Vec<u8>` → JSON array of bytes (NOT hex string).
        // - `gas`, `nonce` are required (Option<u64>); we pass null explicitly
        //   even though serde would default missing fields, to keep the test
        //   shape explicit.
        let req = serde_json::json!({
            "Tx": {
                "chain_id": 1,
                "from": "0x1111111111111111111111111111111111111111",
                "to":   "0x2222222222222222222222222222222222222222",
                "value_wei": "0",
                "data": [0xde, 0xad, 0xbe, 0xef],
                "gas": null,
                "nonce": null
            }
        });
        let out = build_action_json(req.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        // BuildActionResultDto is untagged: either {"other": {...}} (the
        // serialized Action enum) or {"kind":..., "message":...} on err.
        // Envelope wrapping (fix-pass): {"ok":"true","data":<action>}.
        assert_eq!(parsed["ok"], "true");
        assert!(parsed["data"].get("other").is_some(), "expected Action::Other inside data, got {parsed}");
    }
}
```

> JSON-shape facts established from `crates/policy-engine/src/core.rs`:
> - `Request` is externally tagged → wrapper key is `"Tx"` / `"Sig"`.
> - `TransactionRequest` is **snake_case** (`chain_id`, `value_wei`, `data`, `gas`, `nonce`); `data: Vec<u8>` serializes as a JSON byte array.
> - `SignatureRequest` is `rename_all = "camelCase"` (`chainId`, `signer`, `typedData`).
> - `Action::Other` has `#[serde(rename = "other")]`, so `parsed.get("other")` is correct.

- [ ] **Step 4: Run tests**

```bash
cargo test -p policy-engine-wasm build_action 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/policy_engine_wasm/src/
git commit -m "$(cat <<'EOF'
feat(wasm): build_action_json export

Wraps Pipeline::build_action_for behind a JSON boundary. Engine errors
map to {error:{kind,message}} envelopes with stable kind codes
(adapter_ambiguous, adapter_build, lowering_rejected, policy).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `tier1_fact_plan_json` export

**Files:** Modify `crates/policy_engine_wasm/src/exports.rs`, `dto.rs`.

Input: serialized `Action`. Output: serialized `HostFactPlanDto`.

- [ ] **Step 1: Add DTO conversion helpers**

Append to `dto.rs`:

```rust
use policy_engine::core::{Action as EngineAction, OracleRequirementKind, Token};
use policy_engine::lowering::HostFactPlan;

impl From<&Token> for TokenDto {
    fn from(t: &Token) -> Self {
        Self {
            chain_id: t.chain_id,
            address: t.address.as_str().to_string(),
            symbol: t.symbol.clone(),
            decimals: t.decimals,
            is_native: t.is_native,
        }
    }
}

impl From<HostFactPlan> for HostFactPlanDto {
    fn from(plan: HostFactPlan) -> Self {
        Self {
            tokens_for_oracle: plan.tokens_for_oracle.iter().map(TokenDto::from).collect(),
            balances: plan
                .balances
                .into_iter()
                .map(|(owner, token)| BalanceFactDto {
                    owner: owner.as_str().to_string(),
                    token: TokenDto::from(&token),
                })
                .collect(),
            allowances: plan
                .allowances
                .into_iter()
                .map(|(owner, token, spender)| AllowanceFactDto {
                    owner: owner.as_str().to_string(),
                    token: TokenDto::from(&token),
                    spender: spender.as_str().to_string(),
                })
                .collect(),
            clock_required: plan.clock_required,
            sig_oracle_requirements: plan
                .sig_oracle_requirements
                .iter()
                .map(|r| OracleRequirementDto {
                    kind: match r.kind {
                        OracleRequirementKind::Input => "input".into(),
                        OracleRequirementKind::MinOutput => "minOutput".into(),
                    },
                    token: TokenDto::from(&r.token),
                    raw_amount: r.raw_amount.clone(),
                })
                .collect(),
        }
    }
}
```

- [ ] **Step 2: Implement export**

Replace body in `exports.rs`:

```rust
use crate::dto::HostFactPlanDto;
use policy_engine::core::Action;
use policy_engine::lowering::required_host_facts;

#[wasm_bindgen]
pub fn tier1_fact_plan_json(action_json: String) -> String {
    let action: Action = match serde_json::from_str(&action_json) {
        Ok(a) => a,
        Err(e) => return Envelope::<HostFactPlanDto>::err("invalid_action_json", &e.to_string()).to_json(),
    };
    let plan = required_host_facts(&action);
    let dto: HostFactPlanDto = plan.into();
    Envelope::ok(dto).to_json()
}
```

- [ ] **Step 3: Native unit test**

Append:

```rust
#[cfg(test)]
mod tier1_tests {
    use super::*;

    #[test]
    fn tier1_for_dex_returns_input_output_oracle_tokens() {
        // VERIFY: DexAction / DexFacts / Token field renames in core.rs.
        // The plan executor MUST run `grep -B2 "pub struct \(DexAction\|DexFacts\|Token\)" crates/policy-engine/src/core.rs`
        // and adjust this JSON to match. Suspected convention from related types:
        // - `DexAction` and `DexFacts` likely default snake_case (no rename_all);
        // - `Token` likely default snake_case (`chain_id`, `is_native`).
        // Below uses snake_case throughout; flip to camelCase if the actual
        // attrs disagree.
        let action_json = serde_json::json!({
            "dex": {
                "actor":  "0x1111111111111111111111111111111111111111",
                "target": "0xE592427A0AEce92De3Edee1F18E0157C05861564",
                "value_wei": "0",
                "facts": {
                    "protocol_ids": ["uniswap_v3"],
                    "input_tokens": [{
                        "chain_id": 1, "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "symbol": "WETH", "decimals": 18, "is_native": false
                    }],
                    "output_tokens": [{
                        "chain_id": 1, "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC", "decimals": 6, "is_native": false
                    }]
                },
                "oracle_requirements": [],
                "trace": {"steps": []}
            }
        });
        let out = tier1_fact_plan_json(action_json.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        // Envelope wrapping: {"ok":"true","data":<HostFactPlanDto>}.
        assert_eq!(parsed["ok"], "true");
        let data = &parsed["data"];
        assert_eq!(data["tokens_for_oracle"].as_array().unwrap().len(), 2);
        assert_eq!(data["balances"].as_array().unwrap().len(), 1);
        assert_eq!(data["allowances"].as_array().unwrap().len(), 1);
        assert_eq!(data["clock_required"], false);
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p policy-engine-wasm tier1_tests 2>&1 | tail -10
git add crates/policy_engine_wasm/src/
git commit -m "feat(wasm): tier1_fact_plan_json export"
```

---

## Task 6: `tier2_window_keys_json` export

**Files:** Modify `crates/policy_engine_wasm/src/exports.rs`, `dto.rs`.

Input: serialized `Action` + `OracleSnapshot` JSON (subset of `HostSnapshotDto.oracle`). Output: serialized `WindowKeyPlanDto`.

- [ ] **Step 1: Helper to build SnapshotOracle from JSON**

Append to `state.rs`:

```rust
use crate::dto::{HostSnapshotDto, OracleEntryDto};
use policy_engine::core::{Address, Token, UsdValuation};
use policy_engine::host::oracle::SnapshotOracle;

/// Build a [`SnapshotOracle`] from the oracle entries in a host snapshot.
///
/// Each entry's `token_key` must be a chain-qualified address
/// (`{chain_id}:{lowercased_address}`). Decimals/symbol are not needed
/// at lookup time because the oracle keys on `Token::key()` only.
#[must_use]
pub fn snapshot_oracle_from_entries(entries: &[OracleEntryDto]) -> SnapshotOracle {
    let mut oracle = SnapshotOracle::new();
    for entry in entries {
        // Build a placeholder Token whose key matches `entry.token_key`.
        // Engine Oracle::price keys on Token::key() = "{chain_id}:{address.lowercase()}".
        let (chain_str, addr_str) = match entry.token_key.split_once(':') {
            Some(t) => t,
            None => continue,
        };
        let chain_id: u64 = chain_str.parse().unwrap_or(0);
        let address = match Address::new(addr_str) {
            Ok(a) => a,
            Err(_) => continue,
        };
        let token = Token {
            chain_id,
            address,
            symbol: String::new(),
            decimals: 0,
            is_native: false,
        };
        oracle.insert(
            &token,
            UsdValuation {
                value: entry.usd_per_unit.clone(),
                as_of_ts: entry.as_of_ts,
                sources: entry.sources.clone(),
                stale_sec: entry.stale_sec,
            },
        );
    }
    oracle
}
```

- [ ] **Step 2: Implement export**

Replace body in `exports.rs`:

```rust
use crate::dto::WindowKeyPlanDto;
use crate::state::snapshot_oracle_from_entries;
use policy_engine::lowering::required_window_keys;

#[wasm_bindgen]
pub fn tier2_window_keys_json(action_json: String, oracle_snapshot_json: String) -> String {
    let action: Action = match serde_json::from_str(&action_json) {
        Ok(a) => a,
        Err(e) => return Envelope::<WindowKeyPlanDto>::err("invalid_action_json", &e.to_string()).to_json(),
    };
    let entries: Vec<OracleEntryDto> =
        match serde_json::from_str::<Vec<OracleEntryDto>>(&oracle_snapshot_json) {
            Ok(v) => v,
            Err(e) => return Envelope::<WindowKeyPlanDto>::err("invalid_oracle_snapshot_json", &e.to_string()).to_json(),
        };
    let oracle = snapshot_oracle_from_entries(&entries);
    let plan = required_window_keys(&action, &oracle);
    let dto = WindowKeyPlanDto {
        keys: plan
            .keys
            .into_iter()
            .map(|k| crate::dto::WindowKeyDto {
                actor: k.actor.as_str().to_string(),
                // Canonical wire string: StatKey::as_str() → "swapVolumeUsd24h" / "swapCount24h".
                // Field is named `name` for forward compat with future window types
                // that may not be StatKey-typed (e.g. composite keys).
                name: k.key.as_str().to_string(),
            })
            .collect(),
    };
    Envelope::ok(dto).to_json()
}
```

- [ ] **Step 3: Native unit test**

```rust
#[cfg(test)]
mod tier2_tests {
    use super::*;

    #[test]
    fn tier2_for_dex_returns_window_keys() {
        let action_json = serde_json::json!({
            "dex": {
                "actor":  "0x1111111111111111111111111111111111111111",
                "target": "0xE592427A0AEce92De3Edee1F18E0157C05861564",
                "value_wei": "0",
                "facts": {"protocol_ids": [], "input_tokens": [], "output_tokens": []},
                "oracle_requirements": [],
                "trace": {"steps": []}
            }
        });
        let out = tier2_window_keys_json(action_json.to_string(), "[]".into());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        // Envelope wrapping: {"ok":"true","data":{"keys":[...]}}.
        assert_eq!(parsed["ok"], "true");
        let names: Vec<&str> = parsed["data"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["name"].as_str().unwrap())
            .collect();
        // Canonical wire strings — see crates/policy-engine/src/context_keys.rs:155,157
        assert!(names.contains(&"swapVolumeUsd24h"));
        assert!(names.contains(&"swapCount24h"));
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p policy-engine-wasm tier2_tests 2>&1 | tail -10
git add crates/policy_engine_wasm/src/
git commit -m "feat(wasm): tier2_window_keys_json export"
```

---

## Task 7: `evaluate_json` export — full pipeline

**Files:** Modify `crates/policy_engine_wasm/src/exports.rs`, `state.rs`, `dto.rs`.

Input: serialized `Request` + `HostSnapshotDto`. Output: serialized `VerdictDto`. Errors are folded into `Verdict::Fail` with reason codes (never thrown).

- [ ] **Step 1: HostSnapshot → engine HostCapabilities adapter**

Append to `state.rs`. Note: actual engine traits (verified at `host/portfolio.rs:39`, `host/approvals.rs:41`, `host/clock.rs:8`, `host/stat_windows.rs:64`) return `Result<AmountSpec, ...>` / `u64` / `HashMap<StatKey, StatValue>` — not `Option<String>` / `i64`. The snapshot host wraps the JSON-decoded entries to satisfy the real signatures:

```rust
use alloy_primitives::U256;
use policy_engine::core::{Address, AmountSpec, Token};
use policy_engine::host::approvals::{Approvals, ApprovalsError};
use policy_engine::host::clock::Clock;
use policy_engine::host::portfolio::{Portfolio, PortfolioError};
use policy_engine::host::stat_windows::{StatKey, StatValue, StatWindows};
use std::collections::HashMap;

/// Snapshot Portfolio: HashMap<(owner_lower, token_key), raw_amount_decimal_string>.
/// On miss returns Err(PortfolioError::NotFound) so lowering's optional-fact
/// logic kicks in (matches the engine's "missing facts → omitted context"
/// contract).
pub struct SnapshotPortfolio {
    pub balances: HashMap<(String, String), String>,
}
impl Portfolio for SnapshotPortfolio {
    fn balance(&self, owner: &Address, token: &Token) -> Result<AmountSpec, PortfolioError> {
        let key = (owner.as_str().to_lowercase(), token.key());
        let raw_str = self
            .balances
            .get(&key)
            .ok_or(PortfolioError::NotFound)?;
        let raw =
            U256::from_str_radix(raw_str, 10).map_err(|_| PortfolioError::InvalidAmount)?;
        Ok(AmountSpec::from_raw(token.clone(), raw))
    }
}

/// Snapshot Approvals: HashMap<(owner_lower, token_key, spender_lower), raw_allowance>.
pub struct SnapshotApprovals {
    pub allowances: HashMap<(String, String, String), String>,
}
impl Approvals for SnapshotApprovals {
    fn allowance(
        &self,
        owner: &Address,
        token: &Token,
        spender: &Address,
    ) -> Result<AmountSpec, ApprovalsError> {
        let key = (
            owner.as_str().to_lowercase(),
            token.key(),
            spender.as_str().to_lowercase(),
        );
        let raw_str = self
            .allowances
            .get(&key)
            .ok_or(ApprovalsError::NotFound)?;
        let raw =
            U256::from_str_radix(raw_str, 10).map_err(|_| ApprovalsError::InvalidAmount)?;
        Ok(AmountSpec::from_raw(token.clone(), raw))
    }
}

/// FixedClock returns a Unix timestamp (`u64`, NOT `i64`).
pub struct FixedClock(pub u64);
impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

/// Snapshot StatWindows: HashMap<(actor_lower, StatKey), confirmed value>.
/// `snapshot()` returns the engine-consumed `(StatKey, StatValue)` map for the
/// requested actor; absent entries default to zero (StatValue::default()).
pub struct SnapshotStatWindows {
    /// Keyed by (actor_lower, stat_key_str) for cheap JSON ingestion.
    pub windows: HashMap<(String, &'static str), StatValue>,
}
impl StatWindows for SnapshotStatWindows {
    fn snapshot(&self, owner: &Address, keys: &[StatKey]) -> HashMap<StatKey, StatValue> {
        let actor = owner.as_str().to_lowercase();
        let mut out = HashMap::new();
        for k in keys {
            let v = self
                .windows
                .get(&(actor.clone(), k.as_str()))
                .cloned()
                .unwrap_or_default();
            out.insert(*k, v);
        }
        out
    }
}
```

> If `PortfolioError::NotFound` / `ApprovalsError::NotFound` / `InvalidAmount` variants don't exist verbatim, check the actual enum variants and adjust — but the *shapes* (Result<AmountSpec, _>) are confirmed against `crates/policy-engine/src/host/{portfolio,approvals}.rs:39,41` and must be matched.

- [ ] **Step 2: Implement export**

Replace body in `exports.rs`:

```rust
use crate::dto::{HostSnapshotDto, MatchedPolicyDto, VerdictDto};
use crate::state::{
    registry, signature_registry, snapshot_oracle_from_entries, FixedClock,
    SnapshotApprovals, SnapshotPortfolio, SnapshotStatWindows,
};
use policy_engine::host::stat_windows::{StatKey, StatValue};
use policy_engine::host::HostCapabilities;
use policy_engine::policy::Verdict;
use std::collections::HashMap;

#[wasm_bindgen]
pub fn evaluate_json(request_json: String, host_snapshot_json: String) -> String {
    let verdict = (|| -> Result<Verdict, EngineErrorDto> {
        let request: Request = serde_json::from_str(&request_json).map_err(|e| EngineErrorDto {
            kind: "invalid_request_json".into(),
            message: e.to_string(),
        })?;
        let snapshot: HostSnapshotDto =
            serde_json::from_str(&host_snapshot_json).map_err(|e| EngineErrorDto {
                kind: "invalid_host_snapshot_json".into(),
                message: e.to_string(),
            })?;

        let oracle = snapshot_oracle_from_entries(&snapshot.oracle);

        let mut balances = HashMap::new();
        for b in &snapshot.balances {
            balances.insert(
                (b.owner.to_lowercase(), b.token_key.clone()),
                b.balance.clone(),
            );
        }
        let portfolio = SnapshotPortfolio { balances };

        let mut allowances = HashMap::new();
        for a in &snapshot.allowances {
            allowances.insert(
                (
                    a.owner.to_lowercase(),
                    a.token_key.clone(),
                    a.spender.to_lowercase(),
                ),
                a.allowance.clone(),
            );
        }
        let approvals = SnapshotApprovals { allowances };

        // Build the StatWindows snapshot from `snapshot.windows`. This is
        // the missing-link wiring: without `with_stats`, all 24h-window
        // policies would silently evaluate against an absent context. The
        // canonical stat keys (StatKey::SWAP_VOLUME_USD_24H,
        // StatKey::SWAP_COUNT_24H) are matched by their as_str() form.
        let mut windows_map: HashMap<(String, &'static str), StatValue> = HashMap::new();
        for w in &snapshot.windows {
            // Map JSON wire string -> StatKey constant
            let stat_key = match w.name.as_str() {
                s if s == StatKey::SWAP_VOLUME_USD_24H.as_str() => StatKey::SWAP_VOLUME_USD_24H,
                s if s == StatKey::SWAP_COUNT_24H.as_str() => StatKey::SWAP_COUNT_24H,
                _ => continue, // unknown key — ignore (forward-compatible)
            };
            // StatValue::from_decimal_str (or whatever the engine exposes)
            // may differ; for the integer "value" wire shape we parse u128.
            let v: u128 = match w.value.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            windows_map.insert((w.actor.to_lowercase(), stat_key.as_str()), StatValue::from(v));
        }
        let stats = SnapshotStatWindows { windows: windows_map };

        let clock = FixedClock(snapshot.now_ts.unwrap_or(0) as u64);

        let registry = registry();
        let sig_registry = signature_registry();

        STATE.with(|s| {
            let state = s.borrow();
            let state = state.as_ref().ok_or_else(|| EngineErrorDto {
                kind: "not_installed".into(),
                message: "install_policies_json must be called first".into(),
            })?;
            let host = HostCapabilities::new(&oracle)
                .with_clock(&clock)
                .with_portfolio(&portfolio)
                .with_approvals(&approvals)
                .with_stats(&stats); // ← critical: feeds 24h window policies
            let pipeline = Pipeline::new(&registry, host, &state.policies)
                .with_signature_registry(&sig_registry);
            pipeline.evaluate(&request).map_err(|e| EngineErrorDto {
                kind: pipeline_error_kind(&e).into(),
                message: e.to_string(),
            })
        })
    })();

    let dto = match verdict {
        Ok(v) => verdict_to_dto(v),
        Err(e) => VerdictDto::Fail {
            matched: vec![MatchedPolicyDto {
                policy_id: format!("__engine::{}", e.kind),
                reason: Some(e.message),
                severity: "deny".into(),
                origin: "engine_error".into(),
            }],
        },
    };
    // evaluate_json wraps engine errors into Verdict::Fail internally; the
    // outer envelope is therefore always Ok(verdict). Keeps the contract
    // uniform — `unwrap()` in the TS bridge always returns a VerdictDto here.
    Envelope::ok(dto).to_json()
}

fn verdict_to_dto(v: Verdict) -> VerdictDto {
    match v {
        Verdict::Pass => VerdictDto::Pass,
        Verdict::Warn(matched) => VerdictDto::Warn {
            matched: matched.iter().map(matched_to_dto).collect(),
        },
        Verdict::Fail(matched) => VerdictDto::Fail {
            matched: matched.iter().map(matched_to_dto).collect(),
        },
    }
}

fn matched_to_dto(m: &policy_engine::policy::MatchedPolicy) -> MatchedPolicyDto {
    MatchedPolicyDto {
        policy_id: m.policy_id.clone(),
        reason: m.reason.clone(),
        severity: format!("{:?}", m.severity).to_lowercase(),
        origin: format!("{:?}", m.origin).to_lowercase(),
    }
}
```

- [ ] **Step 3: Native end-to-end test**

```rust
#[cfg(test)]
mod evaluate_tests {
    use super::install_tests::minimal_schema;
    use super::*;

    #[test]
    fn evaluate_pass_on_unrecognized_call_with_no_policies() {
        install_policies_json(
            serde_json::json!({"schema_text": minimal_schema(), "policy_set": []}).to_string(),
        );
        let req = serde_json::json!({
            "Tx": {
                "chain_id": 1,
                "from": "0x1111111111111111111111111111111111111111",
                "to":   "0x2222222222222222222222222222222222222222",
                "value_wei": "0",
                "data": [0xde, 0xad, 0xbe, 0xef],
                "gas": null,
                "nonce": null
            }
        });
        let snap = serde_json::json!({
            "oracle": [], "balances": [], "allowances": [],
            "now_ts": 1700000000u64, "windows": []
        });
        let out = evaluate_json(req.to_string(), snap.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        // Envelope wrapping: {"ok":"true","data":<VerdictDto>}.
        assert_eq!(parsed["ok"], "true");
        assert_eq!(parsed["data"]["kind"], "pass", "unexpected verdict: {parsed}");
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p policy-engine-wasm evaluate_tests 2>&1 | tail -15
git add crates/policy_engine_wasm/src/
git commit -m "feat(wasm): evaluate_json end-to-end export"
```

---

## Task 8: wasm-bindgen-test browser harness

**Files:** Create `crates/policy_engine_wasm/tests/web.rs`.

Validates the same exports work under wasm32, not just native test compilation.

- [ ] **Step 1: Write the test**

```rust
//! wasm-bindgen-test smoke tests run under wasm32 in headless Chrome/Firefox.

#![cfg(target_arch = "wasm32")]

use policy_engine_wasm::{
    build_action_json, evaluate_json, install_policies_json, tier1_fact_plan_json,
    tier2_window_keys_json,
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn install_then_build_then_evaluate_round_trip() {
    let schema = include_str!("../../../policy-schema/core.cedarschema");
    let install = serde_json::json!({"schema_text": schema, "policy_set": []});
    let install_out = install_policies_json(install.to_string());
    let install_parsed: serde_json::Value = serde_json::from_str(&install_out).unwrap();
    assert_eq!(install_parsed["ok"], "true");

    let req = serde_json::json!({
        "Tx": {
            "chain_id": 1,
            "from": "0x1111111111111111111111111111111111111111",
            "to":   "0x2222222222222222222222222222222222222222",
            "value_wei": "0",
            "data": [0xde, 0xad, 0xbe, 0xef],
            "gas": null,
            "nonce": null
        }
    });

    let action_out = build_action_json(req.to_string());
    let action_parsed: serde_json::Value = serde_json::from_str(&action_out).unwrap();
    assert_eq!(action_parsed["ok"], "true");
    assert!(action_parsed["data"].get("other").is_some(), "{action_out}");

    let plan_out = tier1_fact_plan_json(action_parsed["data"]["other"].to_string());
    let plan_parsed: serde_json::Value = serde_json::from_str(&plan_out).unwrap();
    assert_eq!(plan_parsed["ok"], "true");
    assert!(plan_parsed["data"].get("tokens_for_oracle").is_some());

    let snap = serde_json::json!({
        "oracle": [], "balances": [], "allowances": [],
        "now_ts": 0u64, "windows": []
    });
    let verdict_out = evaluate_json(req.to_string(), snap.to_string());
    let verdict_parsed: serde_json::Value = serde_json::from_str(&verdict_out).unwrap();
    assert_eq!(verdict_parsed["ok"], "true");
    assert_eq!(verdict_parsed["data"]["kind"], "pass");

    let _ = tier2_window_keys_json(action_parsed["data"]["other"].to_string(), "[]".into());
}
```

> The action returned for an unknown selector is `Action::Other`; it serializes as `{"other": {...}}` because the `Action` enum is `#[serde(rename_all = "...")]` tagged. If naming differs, fix the lookup key.

- [ ] **Step 2: Run under wasm-pack**

```bash
wasm-pack test --headless --chrome crates/policy_engine_wasm 2>&1 | tail -20
```

Expected: `running 1 test` → `test passed`.

If headless Chrome is unavailable in the env: try `--firefox` or `--node` (note: `--node` may not support all wasm features; Chrome is the canonical target).

- [ ] **Step 3: Commit**

```bash
git add crates/policy_engine_wasm/tests/
git commit -m "test(wasm): bindgen-test browser harness for all five exports"
```

---

## Task 9: Production wasm artifact build + CI

**Files:** Create `.github/workflows/wasm.yml`, modify root `README.md` (optional build instructions).

- [ ] **Step 1: Build the production artifact**

```bash
wasm-pack build crates/policy_engine_wasm --target web --release --out-dir pkg --out-name policy_engine_wasm 2>&1 | tail -20
```

Expected: `pkg/policy_engine_wasm_bg.wasm` + `policy_engine_wasm.js` + `policy_engine_wasm.d.ts` produced. Capture the `.wasm` size:

```bash
ls -lh crates/policy_engine_wasm/pkg/policy_engine_wasm_bg.wasm
```

Record the size in commit message — this is the design's "WASM bundle (B)" reality check.

- [ ] **Step 2: Add `.gitignore` entry for the build output**

Append to `.gitignore`:

```
crates/policy_engine_wasm/pkg/
```

- [ ] **Step 3: Create CI workflow**

Create `.github/workflows/wasm.yml`:

```yaml
name: wasm

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - name: Install wasm-pack
        run: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
      - name: Build wasm
        run: wasm-pack build crates/policy_engine_wasm --target web --release
      - name: Run native tests on the wasm crate
        run: cargo test -p policy-engine-wasm
      - name: Run wasm-bindgen-tests in headless Chrome
        run: wasm-pack test --headless --chrome crates/policy_engine_wasm
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/wasm.yml .gitignore
git commit -m "$(cat <<'EOF'
ci(wasm): build artifact + headless wasm-bindgen tests

Adds a GitHub Actions job that builds the WASM bridge via wasm-pack
on every push/PR and runs both the native cargo tests and the
wasm-bindgen-test smoke suite under headless Chrome.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Unified error envelope (added in fix pass)

To resolve the Codex finding that `build_action_json`/`tier1_fact_plan_json` returned `{kind, message}` while `evaluate_json` folded errors into `Verdict::Fail` (forcing TS callers to branch by export name), every export now follows one rule:

> **Every export returns one of two envelopes.** A successful result envelope (`{ok: "true", data: <payload>}`) or an error envelope (`{ok: "false", error: {kind, message}}`). The TS bridge wrapper (`extension/src/background/wasm-bridge.ts`, Plan 5 §4.3.2) reads `parsed.ok` once and either returns `parsed.data` or throws an `EngineError(kind, message)`.

Exception: `evaluate_json` *also* maps engine errors to `Verdict::Fail` with reason `__engine::<kind>` matched-policy entries — but the *outer envelope* still uses `{ok: "true", data: <verdict>}`. This means orchestrator code never has to special-case `evaluate_json` vs the others.

Update the relevant sections of Tasks 3-7 to wrap their existing return values in `{ok, data}` / `{ok, error}` shapes. The internal `Verdict::Fail` reason mapping in Task 7 is unchanged.

## Self-review summary

**Spec coverage** (vs design §4.3.2):
- ✅ `install_policies_json`, `build_action_json`, `tier1_fact_plan_json`, `tier2_window_keys_json`, `evaluate_json` — Tasks 3, 4, 5, 6, 7
- ✅ JSON-string boundary with **unified `{ok, data | error}` envelope** — Task 0a (this fix)
- ✅ Fail-closed: lowering / ambiguous errors fold into `Verdict::Fail` — Task 7
- ✅ **Window state actually plumbed into HostCapabilities via `with_stats(&SnapshotStatWindows)`** — Task 7 (was missing pre-fix; would have silently dead-ended 24h cap policies)
- ✅ wasm-bindgen-test browser harness — Task 8
- ✅ wasm-pack build + CI — Task 9

**Risks flagged for the executor:**
- `PolicyEngineBuilder` API names verified at Task 3 step 2 (a grep before use)
- Engine `Request`/`Action` serde tags verified at Task 4 step 3 (a grep before tests)
- `Portfolio`/`Approvals`/`Clock` trait method signatures verified at Task 7 step 1 (a grep before use)
- WASM bundle size recorded at Task 9 step 1 — input to design §3.2 latency budget validation
