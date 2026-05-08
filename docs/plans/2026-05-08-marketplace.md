# Marketplace + Bundle Parameterization — Plan 6

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Plan 5's hardcoded default policy set with a user-installable marketplace. Bundles ship as `.cedar.tmpl` + `params.schema.json` zips fetched from a static catalog. Install validation enforces (a) signature integrity, (b) sandbox (no schema/adapter/script files), (c) typed parameter rendering, (d) AST-equivalence between template and rendered Cedar so injection attacks fail.

**Architecture:** Catalog is a static GitHub Pages site holding a signed `index.json` and per-bundle zip URLs. Extension fetches the index, verifies the publisher signature (Ed25519 root key pinned in extension), downloads bundles, validates structure, renders templates per typed parameters into final Cedar text, ASTs the template + rendered text and asserts equality at every non-slot node, hands the result to the WASM bridge's `install_policies_json`. UI surface adds a "Marketplace" tab and per-bundle settings.

**Tech Stack:** Native `crypto.subtle` for Ed25519 signature verification (Web Crypto), `@noble/hashes` for sha-256, `jszip` for bundle decompression, `cedar-policy-formatter`/AST access via a thin Rust wasm export added to Plan 2's bridge crate. React for UI.

**Series:** Plan 6 of the Chrome-extension series. Depends on all prior plans. After Plan 6, the extension is feature-complete for v1.

**Scope:** Bundle format, install flow, AST validator, catalog fetch, marketplace + settings UI, replace Plan 5's static install with bundle-driven install.

**Out of scope (v1.1+):** Author key rotation, manifest_version 2 (schema extensions), declarative adapter bundles, bundle reviews/ratings, off-catalog `chrome.runtime.connectExternal` install URLs.

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `extension/package.json` | Modify | Add `jszip`, `@noble/hashes`, `@noble/ed25519` |
| `crates/policy_engine_wasm/src/exports.rs` | Modify | Add `parse_policy_ast_json` export — returns the structural fingerprint of a policy text |
| `crates/policy_engine_wasm/src/dto.rs` | Modify | Add `PolicyAstFingerprintDto` |
| `extension/src/background/marketplace/catalog-client.ts` | Create | Fetch + verify catalog index |
| `extension/src/background/marketplace/bundle-fetch.ts` | Create | Fetch + sha256-verify a bundle zip |
| `extension/src/background/marketplace/bundle-validator.ts` | Create | Sandbox: reject any file outside `policies/`, `manifest.json`, `params.schema.json` |
| `extension/src/background/marketplace/params-validator.ts` | Create | Typed param schema: integer / address / enum / string / array |
| `extension/src/background/marketplace/cedar-literal.ts` | Create | Per-type Cedar literal serializer |
| `extension/src/background/marketplace/template-renderer.ts` | Create | Render `.cedar.tmpl` with typed params; AST-equivalence guard |
| `extension/src/background/marketplace/installer.ts` | Create | Orchestrates fetch → validate → render → check → install |
| `extension/src/background/marketplace/storage.ts` | Create | Installed bundles + per-bundle params in `chrome.storage.local` |
| `extension/src/background/policies-loader.ts` | Modify | Replaces hardcoded default load with installed bundles |
| `extension/src/marketplace/index.tsx` | Create | Marketplace + settings React UI entry |
| `extension/src/marketplace/MarketplacePage.tsx` | Create | Browse / install UI |
| `extension/src/marketplace/SettingsPage.tsx` | Create | Per-bundle parameter editor |
| `extension/public/options.html` | Create | Hosts the marketplace + settings UI |
| `extension/src/manifest.json` | Modify | `options_ui.page` = `options.html` |
| `extension/src/background/__tests__/installer.test.ts` | Create | End-to-end install flow with a fixture bundle |

---

## Task 1: WASM export — policy AST fingerprint

**Files:** Modify `crates/policy_engine_wasm/src/exports.rs`, `dto.rs`.

We need a deterministic structural fingerprint of a Cedar policy that ignores literal values at slot positions. The fingerprint is a JSON tree where every literal becomes a placeholder string `"<slot>"`. Comparing two fingerprints yields AST equivalence with literal values free to vary.

- [ ] **Step 1: Confirm cedar-policy AST surface AND record a golden snapshot**

```bash
cargo doc -p cedar-policy --no-deps --open 2>/dev/null
```

Look for `Policy::ast()` or similar. As of cedar-policy 4.x, `cedar_policy::Policy` exposes `to_json()` which serializes the AST in stable form. Use that.

If `to_json` is not exposed in 4.x, use `cedar_policy_core::ast::Policy` (re-exported via `cedar_policy::Policy`) and walk via the public CST APIs.

**Golden-test gate (added in fix pass)**: before writing any blanker logic, capture the *actual* JSON shape `Policy::to_json()` emits for each Cedar literal kind (long, bool, string, entity ref, decimal, set literal, record literal). Save those snapshots under `crates/policy_engine_wasm/tests/cedar_ast_golden/{long,bool,string,entity,decimal,set,record}.json`. The blanker's literal-key list is then derived *from those snapshots* — never from a guess. The Plan 6 Task 1 unit tests assert that:

1. Every golden snapshot has at least one node whose key matches a blanker entry (so we know we're catching them).
2. Two policies that differ *only at literal positions* of each kind produce identical fingerprints.
3. Two policies that differ in *any structural way* (`when { false || x }` vs `when { x }`, `permit` vs `forbid`, swapped scope, severity-annotation flip) produce different fingerprints.

If the cedar JSON schema changes between releases, the golden snapshots fail loudly and the blanker is updated rather than silently passing/failing legitimate templates.

- [ ] **Step 2: Add the export**

Append to `dto.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PolicyAstFingerprintDto {
    pub policy_id: String,
    pub fingerprint_json: String,
}
```

Append to `exports.rs`:

```rust
use cedar_policy::Policy;

#[wasm_bindgen]
pub fn parse_policy_ast_json(policy_id: String, policy_text: String) -> String {
    let policy = match Policy::parse(Some(policy_id.clone().into()), &policy_text) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&EngineErrorDto {
                kind: "policy_parse".into(),
                message: e.to_string(),
            })
            .expect("serialize");
        }
    };

    // `Policy::to_json()` returns a serde_json::Value of the entire AST.
    // We then walk it and replace every `Lit`/`Value` node with a sentinel
    // so that slot-substituted variants compare equal everywhere except
    // at slot positions.
    let raw = match policy.to_json() {
        Ok(v) => v,
        Err(e) => {
            return serde_json::to_string(&EngineErrorDto {
                kind: "policy_to_json".into(),
                message: e.to_string(),
            })
            .expect("serialize");
        }
    };
    let blanked = blank_literals(raw);
    let dto = PolicyAstFingerprintDto {
        policy_id,
        fingerprint_json: serde_json::to_string(&blanked).unwrap_or_default(),
    };
    serde_json::to_string(&dto).expect("serialize")
}

/// Recursively walk the AST JSON and replace every `Lit` / `Value` /
/// `EntityUid` / `Decimal` / `String` literal with the sentinel `"<slot>"`.
/// Tags (`type`, `op`, `effect`, scope shapes, function names, etc.) stay
/// untouched, so the fingerprint captures policy *structure* only.
fn blank_literals(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Object(mut map) => {
            // Cedar's JSON form uses `Lit`, `Value`, and `__expr` kinds
            // for terminal literal nodes. Replace those subtrees wholesale.
            const LITERAL_KEYS: &[&str] = &["Lit", "Value", "Long", "Bool", "String", "Decimal"];
            for key in LITERAL_KEYS {
                if map.contains_key(*key) {
                    return Value::String("<slot>".into());
                }
            }
            for v in map.values_mut() {
                let taken = std::mem::replace(v, Value::Null);
                *v = blank_literals(taken);
            }
            Value::Object(map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(blank_literals).collect()),
        // Terminal scalars in the engine's emitted JSON are already
        // structural (e.g. operator names like "==", scope keywords).
        other => other,
    }
}
```

> The literal-key list above (`Lit`/`Value`/`Long`/`Bool`/`String`/`Decimal`) is best-effort; the executor must verify against actual cedar-policy 4.x JSON output and adjust. A loose union is safer than a tight one — false positives (extra blanking) only weaken structural specificity, not enable injection.

- [ ] **Step 3: Native test**

Append to `exports.rs`:

```rust
#[cfg(test)]
mod ast_fingerprint_tests {
    use super::*;

    #[test]
    fn same_structure_different_literals_yields_same_fingerprint() {
        let a = parse_policy_ast_json(
            "p::a".into(),
            "permit(principal, action, resource) when { context.totalInputUsd <= 100 };".into(),
        );
        let b = parse_policy_ast_json(
            "p::b".into(),
            "permit(principal, action, resource) when { context.totalInputUsd <= 500 };".into(),
        );
        let pa: serde_json::Value = serde_json::from_str(&a).unwrap();
        let pb: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(pa["fingerprint_json"], pb["fingerprint_json"]);
    }

    #[test]
    fn different_structure_yields_different_fingerprint() {
        let a = parse_policy_ast_json(
            "p::a".into(),
            "permit(principal, action, resource) when { context.totalInputUsd <= 100 };".into(),
        );
        // Injection attempt: same `permit` head, but `when` body extended with `false`.
        let b = parse_policy_ast_json(
            "p::a".into(),
            "permit(principal, action, resource) when { false || (context.totalInputUsd <= 100) };".into(),
        );
        let pa: serde_json::Value = serde_json::from_str(&a).unwrap();
        let pb: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_ne!(pa["fingerprint_json"], pb["fingerprint_json"]);
    }

    #[test]
    fn invalid_text_returns_error_envelope() {
        let r = parse_policy_ast_json("p::bad".into(), "this is not cedar".into());
        let p: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(p["kind"], "policy_parse");
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p policy-engine-wasm ast_fingerprint 2>&1 | tail -10
git add crates/policy_engine_wasm/src/
git commit -m "$(cat <<'EOF'
feat(wasm): parse_policy_ast_json — structural fingerprint export

Returns a JSON tree of the policy AST with every literal value replaced
by a "<slot>" sentinel. Two policies whose text differs only at literal
positions produce identical fingerprints; any structural difference
(injected when-clause, swapped operator, modified scope) produces a
different fingerprint. This is the engine-side primitive the bundle
installer's AST-equivalence check consumes (Plan 6 Task 6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Marketplace deps + scaffolding

**Files:** Modify `extension/package.json`.

- [ ] **Step 1: Add deps**

Add to `dependencies`:

```json
"jszip": "^3.10.1",
"@noble/hashes": "^1.4.0",
"@noble/curves": "^1.4.0",
"canonicalize": "^2.0.0"
```

> Notes:
> - `@noble/curves` IS added explicitly. viem ships its own copy as a transitive dep, but importing through viem's internal paths is unstable across viem minor releases. webpack dedupes between transitive and direct deps when versions match, so the SW bundle effectively has one copy. Locking it explicitly avoids "viem updated, our import broke" failures.
> - `canonicalize` is the RFC 8785 (JCS) implementation; both signer and verifier share it so byte-equality is mechanical. Replaces the hand-rolled `canonicalJson` in both files.

Run `yarn install`.

- [ ] **Step 2: Wire AST fingerprint into the wasm-bridge JS**

In `extension/src/background/wasm-bridge.ts`, append:

```typescript
export async function parsePolicyAst(policyId: string, policyText: string): Promise<string> {
  return (await load()).parse_policy_ast_json(policyId, policyText);
}
```

(Update the `WasmExports` interface accordingly with the same signature.)

- [ ] **Step 3: Commit**

```bash
git add extension/package.json extension/yarn.lock extension/src/background/wasm-bridge.ts
git commit -m "chore(extension): jszip + noble crypto + parsePolicyAst bridge"
```

---

## Task 3: Catalog client (fetch + Ed25519 verify)

**Files:** Create `extension/src/background/marketplace/catalog-client.ts`.

- [ ] **Step 1: Pin a publisher root key**

For development, generate a dev key pair locally:

```bash
mkdir -p extension/scripts/catalog-dev
node -e "
const { ed25519 } = require('@noble/curves/ed25519');
const fs = require('fs');
const sk = ed25519.utils.randomPrivateKey();
const pk = ed25519.getPublicKey(sk);
fs.writeFileSync('extension/scripts/catalog-dev/dev-publisher.sk', Buffer.from(sk).toString('hex'));
fs.writeFileSync('extension/scripts/catalog-dev/dev-publisher.pk', Buffer.from(pk).toString('hex'));
console.log('publisher pubkey hex:', Buffer.from(pk).toString('hex'));
"
```

The `.sk` file must NOT be committed (add to `.gitignore`). Capture the pubkey hex.

```bash
echo "extension/scripts/catalog-dev/*.sk" >> .gitignore
```

- [ ] **Step 2: Write the client**

```typescript
// extension/src/background/marketplace/catalog-client.ts
import { ed25519 } from '@noble/curves/ed25519';
import canonicalize from 'canonicalize';

// Dev publisher key. Production builds replace this constant with a
// pinned root key per release channel.
const PUBLISHER_PUBKEY_HEX =
  process.env.SCOPEBALL_PUBLISHER_PUBKEY ?? '<replace-with-dev-pubkey-from-Step-1>';

export interface CatalogIndex {
  catalog_version: 1;
  signed_at: string;
  signature: string; // hex Ed25519 signature over canonical JSON of the rest
  publisher_pubkey: string; // must match PUBLISHER_PUBKEY_HEX
  bundles: CatalogBundleEntry[];
}

export interface CatalogBundleEntry {
  bundle_id: string;
  version: string; // semver
  manifest_url: string;
  bundle_url: string;
  bundle_sha256: string;
  author: { name: string; pubkey: string };
}

export async function fetchAndVerifyCatalog(
  url: string,
  fetchImpl: typeof fetch = fetch,
): Promise<CatalogIndex> {
  const response = await fetchImpl(url, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(`catalog HTTP ${response.status}`);
  const raw: CatalogIndex = await response.json();

  if (raw.publisher_pubkey.toLowerCase() !== PUBLISHER_PUBKEY_HEX.toLowerCase()) {
    throw new Error(`catalog publisher pubkey mismatch: got ${raw.publisher_pubkey}`);
  }

  const { signature, ...rest } = raw;
  // RFC 8785 canonicalization — shared with sign-catalog.js so byte
  // equality is guaranteed by the same library on both ends.
  const canonical = canonicalize(rest);
  if (canonical === undefined) throw new Error('catalog body did not canonicalize');
  const ok = ed25519.verify(
    hexToBytes(signature),
    new TextEncoder().encode(canonical),
    hexToBytes(raw.publisher_pubkey),
  );
  if (!ok) throw new Error('catalog signature verification failed');
  return raw;
}

function hexToBytes(hex: string): Uint8Array {
  const clean = hex.startsWith('0x') ? hex.slice(2) : hex;
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
}
```

- [ ] **Step 3: Test**

Create `extension/src/background/marketplace/__tests__/catalog-client.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import { ed25519 } from '@noble/curves/ed25519';

const sk = ed25519.utils.randomPrivateKey();
const pk = ed25519.getPublicKey(sk);
const pkHex = Buffer.from(pk).toString('hex');

vi.stubEnv('SCOPEBALL_PUBLISHER_PUBKEY', pkHex);

import { fetchAndVerifyCatalog } from '../catalog-client';

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  const keys = Object.keys(value as object).sort();
  return `{${keys
    .map((k) => `${JSON.stringify(k)}:${canonicalJson((value as any)[k])}`)
    .join(',')}}`;
}

function signedIndex(bundles: any[] = []) {
  const rest = {
    catalog_version: 1 as const,
    signed_at: '2026-05-08T00:00:00Z',
    publisher_pubkey: pkHex,
    bundles,
  };
  const sig = ed25519.sign(new TextEncoder().encode(canonicalJson(rest)), sk);
  return { ...rest, signature: Buffer.from(sig).toString('hex') };
}

describe('fetchAndVerifyCatalog', () => {
  it('accepts a valid signed index', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify(signedIndex())));
    const r = await fetchAndVerifyCatalog('https://catalog.example/index.json', fetchMock as any);
    expect(r.bundles).toEqual([]);
  });

  it('rejects when signature does not verify', async () => {
    const idx = signedIndex();
    idx.signature = '00'.repeat(64);
    const fetchMock = vi.fn(async () => new Response(JSON.stringify(idx)));
    await expect(
      fetchAndVerifyCatalog('https://catalog.example/index.json', fetchMock as any),
    ).rejects.toThrow(/signature/);
  });
});
```

- [ ] **Step 4: Run + commit**

```bash
cd extension && yarn test catalog-client 2>&1 | tail -10
git add extension/src/background/marketplace/catalog-client.ts extension/src/background/marketplace/__tests__/ .gitignore
git commit -m "feat(extension): catalog client (fetch + Ed25519 signature verify)"
```

---

## Task 4: Bundle fetch + sandbox validation

**Files:** Create `extension/src/background/marketplace/bundle-fetch.ts`, `bundle-validator.ts`.

- [ ] **Step 1: bundle-fetch.ts**

```typescript
import { sha256 } from '@noble/hashes/sha256';
import JSZip from 'jszip';

export interface FetchedBundle {
  manifest: BundleManifest;
  policyTemplates: { id: string; text: string }[];
  paramsSchemaJson: string;
}

export interface BundleManifest {
  manifest_version: 1;
  bundle_id: string;
  version: string;
  author: { name: string; pubkey: string };
  policies: { id: string; file: string; default_severity: 'deny' | 'warn' }[];
  params_schema: string;
}

export async function fetchBundle(
  url: string,
  expectedSha256: string,
  fetchImpl: typeof fetch = fetch,
): Promise<FetchedBundle> {
  const response = await fetchImpl(url, { signal: AbortSignal.timeout(15_000) });
  if (!response.ok) throw new Error(`bundle HTTP ${response.status}`);
  const buf = new Uint8Array(await response.arrayBuffer());

  const actual = bytesToHex(sha256(buf));
  if (actual.toLowerCase() !== expectedSha256.toLowerCase()) {
    throw new Error(`bundle sha256 mismatch: got ${actual}, expected ${expectedSha256}`);
  }

  const zip = await JSZip.loadAsync(buf);
  const manifestEntry = zip.file('manifest.json');
  if (!manifestEntry) throw new Error('bundle missing manifest.json');
  const manifest: BundleManifest = JSON.parse(await manifestEntry.async('string'));
  if (manifest.manifest_version !== 1) {
    throw new Error(`unsupported manifest_version ${manifest.manifest_version}`);
  }

  const paramsSchemaEntry = zip.file(manifest.params_schema);
  if (!paramsSchemaEntry) throw new Error(`bundle missing params schema ${manifest.params_schema}`);
  const paramsSchemaJson = await paramsSchemaEntry.async('string');

  const policyTemplates: { id: string; text: string }[] = [];
  for (const policyDecl of manifest.policies) {
    const entry = zip.file(policyDecl.file);
    if (!entry) throw new Error(`bundle missing policy file ${policyDecl.file}`);
    policyTemplates.push({ id: policyDecl.id, text: await entry.async('string') });
  }
  return { manifest, policyTemplates, paramsSchemaJson };
}

function bytesToHex(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i++) s += bytes[i].toString(16).padStart(2, '0');
  return s;
}
```

- [ ] **Step 2: bundle-validator.ts (sandbox)**

```typescript
import JSZip from 'jszip';

const ALLOWED_TOPLEVEL = new Set(['manifest.json', 'params.schema.json', 'README.md', 'LICENSE']);
// Strict: any direct child of policies/ must be `<name>.cedar.tmpl`. No
// nested directories, no other extensions, no name-only files.
const POLICY_PATH_RE = /^policies\/[A-Za-z0-9_.-]+\.cedar\.tmpl$/;

export function validateBundleSandbox(zip: JSZip): void {
  for (const [path, file] of Object.entries(zip.files)) {
    if (file.dir) continue;
    if (path.endsWith('/')) continue;
    if (path.includes('..') || path.startsWith('/')) {
      throw new Error(`bundle contains path traversal: ${path}`);
    }
    if (POLICY_PATH_RE.test(path)) continue;
    if (ALLOWED_TOPLEVEL.has(path)) continue;
    throw new Error(
      `bundle violates sandbox: file "${path}" not in policies/<name>.cedar.tmpl, manifest.json, params.schema.json, README.md, or LICENSE`,
    );
  }
}

/// Manifest-level invariants enforced at install time:
/// - `params_schema` field must literally equal "params.schema.json"
/// - every `policies[].file` must match the policies/*.cedar.tmpl shape
/// Catches bundles that satisfy the file-level sandbox but try to point
/// the manifest at e.g. README.md as their schema.
export function validateBundleManifestPaths(manifest: { params_schema: string; policies: { file: string }[] }): void {
  if (manifest.params_schema !== 'params.schema.json') {
    throw new Error(`manifest.params_schema must be "params.schema.json", got "${manifest.params_schema}"`);
  }
  for (const p of manifest.policies) {
    if (!POLICY_PATH_RE.test(p.file)) {
      throw new Error(`manifest references non-policy file: "${p.file}"`);
    }
  }
}
```

> Wire `validateBundleSandbox(zip)` into `fetchBundle` immediately after `zip = await JSZip.loadAsync(...)`.

- [ ] **Step 3: Test**

Create `extension/src/background/marketplace/__tests__/bundle-validator.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import JSZip from 'jszip';
import { validateBundleSandbox } from '../bundle-validator';

async function makeZip(files: Record<string, string>): Promise<JSZip> {
  const zip = new JSZip();
  for (const [k, v] of Object.entries(files)) zip.file(k, v);
  // Re-load to mimic the loadAsync path.
  return JSZip.loadAsync(await zip.generateAsync({ type: 'uint8array' }));
}

describe('validateBundleSandbox', () => {
  it('accepts the canonical bundle layout', async () => {
    const zip = await makeZip({
      'manifest.json': '{}',
      'params.schema.json': '{}',
      'policies/foo.cedar.tmpl': 'permit(...);',
      'README.md': '# bundle',
    });
    expect(() => validateBundleSandbox(zip)).not.toThrow();
  });

  it('rejects schema fragments', async () => {
    const zip = await makeZip({
      'manifest.json': '{}',
      'params.schema.json': '{}',
      'schema-extensions/x.cedarschema': 'entity Foo;',
    });
    expect(() => validateBundleSandbox(zip)).toThrow(/sandbox/);
  });

  it('rejects path traversal', async () => {
    const zip = await makeZip({
      'manifest.json': '{}',
      'params.schema.json': '{}',
      '../etc/passwd': 'evil',
    });
    expect(() => validateBundleSandbox(zip)).toThrow(/sandbox|path traversal/);
  });

  it('rejects scripts', async () => {
    const zip = await makeZip({
      'manifest.json': '{}',
      'params.schema.json': '{}',
      'policies/install.js': 'window.alert("pwned")',
    });
    expect(() => validateBundleSandbox(zip)).toThrow(/sandbox/);
  });
});
```

- [ ] **Step 4: Run + commit**

```bash
cd extension && yarn test bundle-validator 2>&1 | tail -10
git add extension/src/background/marketplace/bundle-fetch.ts extension/src/background/marketplace/bundle-validator.ts extension/src/background/marketplace/__tests__/bundle-validator.test.ts
git commit -m "feat(extension): bundle fetch (sha256 + sandbox)"
```

---

## Task 5: Typed parameter validator + Cedar literal serializer

**Files:** Create `extension/src/background/marketplace/params-validator.ts`, `cedar-literal.ts`.

- [ ] **Step 1: params-validator.ts**

```typescript
/// Each ParamSchema variant carries an optional `default` value of the right
/// type. The marketplace UI uses defaults when installing without a per-param
/// edit pass; SettingsPage edits the active values post-install.
export type ParamSchema =
  | { type: 'integer'; min: number; max: number; default?: number }
  | { type: 'address'; default?: string }
  | { type: 'enum'; values: readonly string[]; default?: string }
  | { type: 'string'; maxLen: number; allowedChars: string; default?: string }
  | { type: 'array'; items: ParamSchema; maxItems: number; default?: unknown[] };

export type ParamsSchema = Record<string, ParamSchema>;
export type ParamValues = Record<string, unknown>;

/// Build a ParamValues object from a schema's defaults. Throws if any param
/// lacks a default and no override is supplied. The MarketplacePage uses
/// this to install bundles without forcing the user through an edit form
/// before the bundle is even on disk; SettingsPage edits afterward.
export function defaultsFor(schema: ParamsSchema, overrides: ParamValues = {}): ParamValues {
  const out: ParamValues = { ...overrides };
  for (const [key, decl] of Object.entries(schema)) {
    if (key in out) continue;
    if (decl.default === undefined) {
      throw new Error(
        `param "${key}" has no default and was not supplied at install time`,
      );
    }
    out[key] = decl.default;
  }
  return out;
}

const ADDRESS_RE = /^0x[a-fA-F0-9]{40}$/;

export function validateParams(schema: ParamsSchema, values: ParamValues): void {
  for (const [key, decl] of Object.entries(schema)) {
    if (!(key in values)) throw new Error(`param missing: ${key}`);
    validateOne(`${key}`, decl, values[key]);
  }
  for (const key of Object.keys(values)) {
    if (!(key in schema)) throw new Error(`param not declared in schema: ${key}`);
  }
}

function validateOne(path: string, decl: ParamSchema, value: unknown): void {
  switch (decl.type) {
    case 'integer': {
      if (typeof value !== 'number' || !Number.isInteger(value)) {
        throw new Error(`${path}: expected integer, got ${typeof value}`);
      }
      if (value < decl.min || value > decl.max) {
        throw new Error(`${path}: ${value} outside [${decl.min}, ${decl.max}]`);
      }
      return;
    }
    case 'address': {
      if (typeof value !== 'string' || !ADDRESS_RE.test(value)) {
        throw new Error(`${path}: expected 0x-prefixed 40-char hex address`);
      }
      return;
    }
    case 'enum': {
      if (typeof value !== 'string' || !decl.values.includes(value)) {
        throw new Error(`${path}: must be one of ${decl.values.join(',')}`);
      }
      return;
    }
    case 'string': {
      if (typeof value !== 'string') throw new Error(`${path}: expected string`);
      if (value.length > decl.maxLen) throw new Error(`${path}: length > ${decl.maxLen}`);
      const allowed = new RegExp(`^[${escapeForCharClass(decl.allowedChars)}]*$`);
      if (!allowed.test(value)) throw new Error(`${path}: contains disallowed characters`);
      return;
    }
    case 'array': {
      if (!Array.isArray(value)) throw new Error(`${path}: expected array`);
      if (value.length > decl.maxItems) throw new Error(`${path}: more than ${decl.maxItems} items`);
      value.forEach((item, i) => validateOne(`${path}[${i}]`, decl.items, item));
      return;
    }
  }
}

function escapeForCharClass(s: string): string {
  return s.replace(/[\\\]^-]/g, '\\$&');
}
```

- [ ] **Step 2: cedar-literal.ts**

```typescript
import type { ParamSchema } from './params-validator';

const INTEGER_RE = /^-?[0-9]+$/;

/// Render a typed param value into Cedar literal syntax. Each call must
/// re-validate against the regex appropriate to the type — defense-in-depth
/// over the schema validator.
export function renderCedarLiteral(decl: ParamSchema, value: unknown): string {
  switch (decl.type) {
    case 'integer': {
      const s = String(value);
      if (!INTEGER_RE.test(s)) throw new Error(`integer regex fail: ${s}`);
      return s;
    }
    case 'address': {
      // The repo schema (policy-schema/core.cedarschema) does NOT declare
      // an `EthereumAddress` entity type — addresses are plain `String`
      // fields throughout the action contexts. Render as a Cedar string
      // literal. (Future: introduce a dedicated entity type and migrate
      // contexts; out of v1 scope.)
      const v = String(value);
      if (!/^0x[a-fA-F0-9]{40}$/.test(v)) throw new Error(`address regex fail: ${v}`);
      return JSON.stringify(v.toLowerCase());
    }
    case 'enum': {
      const v = String(value);
      if (!decl.values.includes(v)) throw new Error(`enum value fail: ${v}`);
      // Treat enum values as Cedar string literals for now. Future: dedicated
      // entity types per enum domain.
      return JSON.stringify(v);
    }
    case 'string': {
      const v = String(value);
      // JSON.stringify escapes ", \, control chars correctly for Cedar string literals.
      return JSON.stringify(v);
    }
    case 'array': {
      const arr = value as unknown[];
      const inner = arr.map((item) => renderCedarLiteral(decl.items, item)).join(', ');
      return `[${inner}]`;
    }
  }
}
```

- [ ] **Step 3: Tests**

Create `extension/src/background/marketplace/__tests__/cedar-literal.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { renderCedarLiteral } from '../cedar-literal';
import { validateParams, type ParamsSchema } from '../params-validator';

describe('cedar-literal renderer', () => {
  it('renders integer as plain digits', () => {
    expect(renderCedarLiteral({ type: 'integer', min: 0, max: 100 }, 42)).toBe('42');
  });

  it('renders address as a Cedar string literal (lowercased)', () => {
    // Repo schema (policy-schema/core.cedarschema) does not declare an
    // EthereumAddress entity type — addresses are plain String fields,
    // so the renderer emits a quoted, lowercased string.
    expect(
      renderCedarLiteral(
        { type: 'address' },
        '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2',
      ),
    ).toBe('"0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"');
  });

  it('escapes string literals via JSON.stringify', () => {
    const literal = renderCedarLiteral({ type: 'string', maxLen: 64, allowedChars: 'A-Za-z' }, 'hello "world"');
    expect(literal).toBe('"hello \\"world\\""');
  });

  it('rejects integer-injection attempts', () => {
    expect(() =>
      renderCedarLiteral({ type: 'integer', min: 0, max: 999 }, '100 } } forbid' as any),
    ).toThrow();
  });

  it('renders array as Cedar set literal', () => {
    const out = renderCedarLiteral(
      { type: 'array', items: { type: 'integer', min: 0, max: 100 }, maxItems: 5 },
      [1, 2, 3],
    );
    expect(out).toBe('[1, 2, 3]');
  });
});

describe('validateParams', () => {
  it('rejects undeclared params', () => {
    const schema: ParamsSchema = { x: { type: 'integer', min: 0, max: 1 } };
    expect(() => validateParams(schema, { x: 1, y: 2 })).toThrow(/not declared/);
  });

  it('rejects out-of-range integers', () => {
    const schema: ParamsSchema = { x: { type: 'integer', min: 0, max: 10 } };
    expect(() => validateParams(schema, { x: 999 })).toThrow(/outside/);
  });
});
```

- [ ] **Step 4: Run + commit**

```bash
cd extension && yarn test 'cedar-literal|params-validator' 2>&1 | tail -10
git add extension/src/background/marketplace/params-validator.ts extension/src/background/marketplace/cedar-literal.ts extension/src/background/marketplace/__tests__/cedar-literal.test.ts
git commit -m "feat(extension): typed param schema + Cedar literal serializer"
```

---

## Task 6: Template renderer + AST equivalence guard

**Files:** Create `extension/src/background/marketplace/template-renderer.ts`.

Template syntax: `{{paramName}}` placeholders only. The renderer:
1. Parses the template — replacing each `{{x}}` with a per-param sentinel `__SCOPEBALL_SLOT_x__` to get a clean Cedar text.
2. Computes the AST fingerprint of the sentinel-substituted text.
3. Renders the actual user param values via `renderCedarLiteral`, producing the final Cedar text.
4. Computes the AST fingerprint of the final text.
5. Asserts the two fingerprints are identical. Any structural divergence — `when { false }` injection, head modification, severity flip — fails this check.

- [ ] **Step 1: Write the renderer**

```typescript
// extension/src/background/marketplace/template-renderer.ts
import { parsePolicyAst } from '@background/wasm-bridge';
import { renderCedarLiteral } from './cedar-literal';
import type { ParamSchema, ParamsSchema, ParamValues } from './params-validator';

const PLACEHOLDER_RE = /\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}/g;

/// Regex anchors that bound a legal slot position in template text.
/// A `{{x}}` placeholder MUST appear immediately to the right of one of:
/// - a comparison operator (`==`, `!=`, `<`, `<=`, `>`, `>=`)
/// - the `in` keyword
/// - inside a `Set` / `Record` literal `[...]` or `{...}`
///
/// This prevents an attacker from making the entire `when` body the slot
/// (`when { {{cap}} }`) — Codex's documented Plan 6 limitation. The check
/// inspects the characters immediately preceding the placeholder; if no
/// anchor matches, install fails up-front with a clear error.
const ALLOWED_PRECEDING = [
  /==\s*$/,
  /!=\s*$/,
  /<=\s*$/,
  />=\s*$/,
  /<\s*$/,
  />\s*$/,
  /\bin\s+$/,
  /[,\[]\s*$/, // inside set / array literal
];

export interface RenderInput {
  policyId: string;
  templateText: string;
  paramsSchema: ParamsSchema;
  paramValues: ParamValues;
}

/// Walk the template, locate each `{{name}}`, and assert the preceding
/// characters match a whitelisted Cedar context. Throws on any violation.
function assertSlotPositions(template: string): void {
  // Strip line + block comments first so we don't false-match inside them.
  const stripped = template
    .replace(/\/\/[^\n]*/g, '')
    .replace(/\/\*[\s\S]*?\*\//g, '');
  for (const match of stripped.matchAll(PLACEHOLDER_RE)) {
    const before = stripped.slice(0, match.index ?? 0);
    if (!ALLOWED_PRECEDING.some((rx) => rx.test(before))) {
      throw new Error(
        `slot {{${match[1]}}} appears in a non-whitelisted position. Slots may only fill ` +
          `the right-hand side of comparison operators (==, !=, <, >, <=, >=), the right ` +
          `operand of \`in\`, or elements of a set/record literal.`,
      );
    }
  }
}

export async function renderAndVerify(input: RenderInput): Promise<string> {
  // Layer 0 (added in fix pass): structural slot-position whitelist.
  // Rejects templates where a slot fills an unsafe AST position (e.g., the
  // entire `when` body). Cheap defense-in-depth before AST equivalence.
  assertSlotPositions(input.templateText);

  const sentinelText = substituteSentinels(input.templateText, input.paramsSchema);
  const sentinelFingerprint = await fingerprintOf(input.policyId, sentinelText);

  const finalText = substituteFinal(input.templateText, input.paramsSchema, input.paramValues);
  const finalFingerprint = await fingerprintOf(input.policyId, finalText);

  if (sentinelFingerprint !== finalFingerprint) {
    throw new Error(
      `bundle template AST mismatch for ${input.policyId}: rendered text changes structure. Refusing to install.`,
    );
  }
  return finalText;
}

function substituteSentinels(template: string, schema: ParamsSchema): string {
  return template.replace(PLACEHOLDER_RE, (_match, name: string) => {
    const decl = schema[name];
    if (!decl) throw new Error(`template references undeclared param ${name}`);
    return sentinelLiteral(decl, name);
  });
}

function substituteFinal(template: string, schema: ParamsSchema, values: ParamValues): string {
  return template.replace(PLACEHOLDER_RE, (_match, name: string) => {
    const decl = schema[name];
    if (!decl) throw new Error(`template references undeclared param ${name}`);
    if (!(name in values)) throw new Error(`param missing at render time: ${name}`);
    return renderCedarLiteral(decl, values[name]);
  });
}

/// A sentinel literal that has the same Cedar type as the param. The AST
/// fingerprint blanks all literals to "<slot>", so the actual sentinel
/// value is irrelevant — but the literal *kind* must match (integer vs
/// string vs entity ref) so the parsed AST is structurally identical to
/// what the user-rendered version produces.
function sentinelLiteral(decl: ParamSchema, _name: string): string {
  switch (decl.type) {
    case 'integer':
      return '0';
    case 'address':
      return JSON.stringify('0x0000000000000000000000000000000000000000');
    case 'enum':
    case 'string':
      return '"x"';
    case 'array':
      // For arrays, render with one zero-arg sentinel so the array node has
      // the right kind. The fingerprint blanks element literals; structure
      // is what matters.
      return `[${sentinelLiteral(decl.items, _name)}]`;
  }
}

async function fingerprintOf(policyId: string, text: string): Promise<string> {
  const raw = await parsePolicyAst(policyId, text);
  const parsed = JSON.parse(raw);
  if (parsed.kind === 'policy_parse') {
    throw new Error(`policy parse failed: ${parsed.message}`);
  }
  return parsed.fingerprint_json as string;
}
```

- [ ] **Step 2: Tests**

Create `extension/src/background/marketplace/__tests__/template-renderer.test.ts`:

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('@background/wasm-bridge', () => ({
  parsePolicyAst: vi.fn(async (id: string, text: string) => {
    // Naive but-stable fingerprint stub: strip every contiguous run of
    // digits / quoted strings / hex addresses, then return the remainder.
    const blanked = text
      .replace(/-?\d+/g, '<slot>')
      .replace(/"[^"]*"/g, '<slot>')
      .replace(/0x[a-fA-F0-9]+/g, '<slot>');
    return JSON.stringify({
      policy_id: id,
      fingerprint_json: blanked,
    });
  }),
}));

import { renderAndVerify } from '../template-renderer';

describe('renderAndVerify', () => {
  it('renders integer template + verifies AST equivalence', async () => {
    const finalText = await renderAndVerify({
      policyId: 'demo::cap',
      templateText: 'permit(principal, action, resource) when { context.totalInputUsd <= {{cap}} };',
      paramsSchema: { cap: { type: 'integer', min: 1, max: 1000 } },
      paramValues: { cap: 250 },
    });
    expect(finalText).toContain('250');
  });

  it('rejects an injection attempt where the slot is the entire predicate', async () => {
    // Layer 0 (slot-position whitelist) blocks this: {{cap}} is preceded
    // only by `{ ` (block opener), which is not in ALLOWED_PRECEDING.
    // Production behavior: assertSlotPositions throws before AST diffing.
    const evilTemplate =
      'permit(principal, action, resource) when { {{cap}} };'; // user param is the entire predicate
    await expect(
      renderAndVerify({
        policyId: 'demo::evil',
        templateText: evilTemplate,
        paramsSchema: { cap: { type: 'integer', min: 0, max: 1 } },
        paramValues: { cap: 0 },
      }),
    ).rejects.toThrow(/non-whitelisted position/);
  });

  it('accepts a slot in a comparison RHS', async () => {
    await expect(
      renderAndVerify({
        policyId: 'demo::cap-compare',
        templateText:
          'permit(principal, action, resource) when { context.totalInputUsd <= {{cap}} };',
        paramsSchema: { cap: { type: 'integer', min: 1, max: 1000 } },
        paramValues: { cap: 250 },
      }),
    ).resolves.toContain('250');
  });
});
```

> The naive stub is intentionally weak. The integration test in Task 9 runs the real WASM-backed fingerprint and verifies injection rejection.

- [ ] **Step 3: Run + commit**

```bash
cd extension && yarn test template-renderer 2>&1 | tail -10
git add extension/src/background/marketplace/template-renderer.ts extension/src/background/marketplace/__tests__/template-renderer.test.ts
git commit -m "feat(extension): bundle template renderer + AST equivalence guard"
```

---

## Task 7: Installed-bundles storage + installer

**Files:** Create `extension/src/background/marketplace/storage.ts`, `installer.ts`.

- [ ] **Step 1: storage.ts**

```typescript
import Browser from 'webextension-polyfill';

const KEY = 'marketplace:bundles';

export interface InstalledBundle {
  bundle_id: string;
  version: string;
  author_pubkey: string;
  paramValues: Record<string, unknown>;
  renderedPolicySet: { id: string; text: string }[];
  installedAtMs: number;
}

export async function listInstalled(): Promise<InstalledBundle[]> {
  const v = (await Browser.storage.local.get(KEY))[KEY] as InstalledBundle[] | undefined;
  return v ?? [];
}

export async function upsert(bundle: InstalledBundle): Promise<void> {
  const list = await listInstalled();
  const i = list.findIndex((b) => b.bundle_id === bundle.bundle_id);
  if (i >= 0) {
    // First-install pubkey pinning: refuse if pubkey differs.
    if (list[i].author_pubkey !== bundle.author_pubkey) {
      throw new Error(
        `bundle ${bundle.bundle_id} previously installed under a different author pubkey; refuse update`,
      );
    }
    list[i] = bundle;
  } else {
    list.push(bundle);
  }
  await Browser.storage.local.set({ [KEY]: list });
}

export async function uninstall(bundleId: string): Promise<void> {
  const list = await listInstalled();
  await Browser.storage.local.set({ [KEY]: list.filter((b) => b.bundle_id !== bundleId) });
}

export async function aggregatedPolicySet(): Promise<{ id: string; text: string }[]> {
  const list = await listInstalled();
  return list.flatMap((b) => b.renderedPolicySet);
}
```

- [ ] **Step 2: installer.ts**

```typescript
// extension/src/background/marketplace/installer.ts
import { fetchAndVerifyCatalog, type CatalogBundleEntry } from './catalog-client';
import { fetchBundle } from './bundle-fetch';
import { renderAndVerify } from './template-renderer';
import {
  defaultsFor,
  validateParams,
  type ParamsSchema,
  type ParamValues,
} from './params-validator';
import { upsert, type InstalledBundle } from './storage';

const CATALOG_URL = process.env.SCOPEBALL_CATALOG_URL ?? 'https://catalog.scopeball.dev/index.json';

export async function listAvailable(): Promise<CatalogBundleEntry[]> {
  const idx = await fetchAndVerifyCatalog(CATALOG_URL);
  return idx.bundles;
}

export async function installBundle(
  bundle: CatalogBundleEntry,
  paramOverrides: ParamValues = {},
): Promise<void> {
  const fetched = await fetchBundle(bundle.bundle_url, bundle.bundle_sha256);
  if (fetched.manifest.bundle_id !== bundle.bundle_id) {
    throw new Error(`bundle_id mismatch: catalog ${bundle.bundle_id}, manifest ${fetched.manifest.bundle_id}`);
  }
  if (fetched.manifest.author.pubkey !== bundle.author.pubkey) {
    throw new Error('bundle author pubkey mismatch between catalog and manifest');
  }

  const paramsSchema: ParamsSchema = JSON.parse(fetched.paramsSchemaJson);
  // Apply schema defaults; throws if any param has no default AND no override.
  const paramValues = defaultsFor(paramsSchema, paramOverrides);
  validateParams(paramsSchema, paramValues);

  const renderedPolicySet: { id: string; text: string }[] = [];
  for (const tmpl of fetched.policyTemplates) {
    const namespacedId = `${bundle.bundle_id}::${tmpl.id}`;
    const finalText = await renderAndVerify({
      policyId: namespacedId,
      templateText: tmpl.text,
      paramsSchema,
      paramValues,
    });
    renderedPolicySet.push({ id: namespacedId, text: finalText });
  }

  const record: InstalledBundle = {
    bundle_id: bundle.bundle_id,
    version: bundle.version,
    author_pubkey: bundle.author.pubkey,
    paramValues,
    renderedPolicySet,
    installedAtMs: Date.now(),
  };
  await upsert(record);
}
```

- [ ] **Step 3: policies-loader.ts swap**

Replace the body of `extension/src/background/policies-loader.ts`:

```typescript
import Browser from 'webextension-polyfill';
import { aggregatedPolicySet } from './marketplace/storage';
import { installPolicies } from './wasm-bridge';

let installed = false;
let inflight: Promise<void> | null = null;

export async function ensureDefaultPoliciesInstalled(): Promise<void> {
  if (installed) return;
  if (inflight) return inflight;
  inflight = (async () => {
    const schemaUrl = Browser.runtime.getURL('default-policies/schema.cedarschema');
    const schemaText = await (await fetch(schemaUrl)).text();
    const policySet = await aggregatedPolicySet();
    const result = await installPolicies(
      JSON.stringify({ schema_text: schemaText, policy_set: policySet }),
    );
    const parsed = JSON.parse(result);
    if (parsed.ok !== 'true') throw new Error(`installPolicies failed: ${parsed.error}`);
    installed = true;
  })();
  return inflight;
}

/// Forces a re-install (after marketplace install/uninstall/edit).
export async function reinstallAllPolicies(): Promise<void> {
  installed = false;
  inflight = null;
  await ensureDefaultPoliciesInstalled();
}
```

- [ ] **Step 4: Commit**

```bash
git add extension/src/background/marketplace/storage.ts extension/src/background/marketplace/installer.ts extension/src/background/policies-loader.ts
git commit -m "$(cat <<'EOF'
feat(extension): marketplace installer + installed-bundles storage

installBundle: fetch (with sha256) → sandbox-validate → params validate
→ template render-and-verify (AST equivalence) → upsert. Bundle ids are
namespaced (`<bundle>::<policy>`) to prevent collision. First-install
pubkey pinning rejects subsequent updates from a different author key.

policies-loader switches from the static default-policies/policy-set.json
to the user's aggregated installed bundles. reinstallAllPolicies()
re-runs install_policies_json after any marketplace mutation.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Marketplace + settings UI

**Files:** Create `extension/public/options.html`, `extension/src/marketplace/{index.tsx, MarketplacePage.tsx, SettingsPage.tsx, styles.css}`. Modify `manifest.json`, `webpack.common.js`.

- [ ] **Step 1: options.html**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Scopeball — Marketplace & Settings</title>
  </head>
  <body>
    <div id="root"></div>
    <script src="js/marketplace/index.js"></script>
  </body>
</html>
```

- [ ] **Step 2: Wire entry + manifest**

Edit `webpack.common.js` `entry`:

```javascript
'marketplace/index': path.join(sourceDir, 'marketplace', 'index.tsx'),
```

Edit `manifest.json` (top-level, with chrome/firefox keys):

```json
"__chrome__options_ui": { "page": "options.html", "open_in_tab": true },
"__firefox__options_ui": { "page": "options.html", "open_in_tab": true }
```

- [ ] **Step 3: index.tsx**

```tsx
import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { MarketplacePage } from './MarketplacePage';
import { SettingsPage } from './SettingsPage';
import './styles.css';

function App(): JSX.Element {
  const [tab, setTab] = useState<'marketplace' | 'settings'>('marketplace');
  return (
    <div className="root">
      <nav>
        <button className={tab === 'marketplace' ? 'active' : ''} onClick={() => setTab('marketplace')}>
          Marketplace
        </button>
        <button className={tab === 'settings' ? 'active' : ''} onClick={() => setTab('settings')}>
          Installed
        </button>
      </nav>
      {tab === 'marketplace' ? <MarketplacePage /> : <SettingsPage />}
    </div>
  );
}

createRoot(document.getElementById('root')!).render(<App />);
```

- [ ] **Step 4: MarketplacePage.tsx**

```tsx
import React, { useEffect, useState } from 'react';
import { installBundle, listAvailable } from '@background/marketplace/installer';
import type { CatalogBundleEntry } from '@background/marketplace/catalog-client';

export function MarketplacePage(): JSX.Element {
  const [bundles, setBundles] = useState<CatalogBundleEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const list = await listAvailable();
        setBundles(list);
      } catch (e: any) {
        setError(e.message ?? String(e));
      }
    })();
  }, []);

  async function onInstall(b: CatalogBundleEntry): Promise<void> {
    setBusy(b.bundle_id);
    setError(null);
    try {
      // installBundle will read the bundle's params.schema.json and apply
      // `defaultsFor(schema)` internally — install fails up-front if any
      // param has no `default` set, surfacing a clean UI error rather than
      // a confusing later validateParams failure.
      await installBundle(b);
    } catch (e: any) {
      setError(e.message ?? String(e));
    } finally {
      setBusy(null);
    }
  }

  if (error) return <p className="error">Error: {error}</p>;
  return (
    <ul className="bundles">
      {bundles.map((b) => (
        <li key={b.bundle_id}>
          <h3>{b.bundle_id}</h3>
          <small>v{b.version} • {b.author.name}</small>
          <button disabled={busy === b.bundle_id} onClick={() => onInstall(b)}>
            {busy === b.bundle_id ? 'Installing…' : 'Install'}
          </button>
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 5: SettingsPage.tsx**

```tsx
import React, { useEffect, useState } from 'react';
import { listInstalled, uninstall, type InstalledBundle } from '@background/marketplace/storage';
import { reinstallAllPolicies } from '@background/policies-loader';

export function SettingsPage(): JSX.Element {
  const [installed, setInstalled] = useState<InstalledBundle[]>([]);

  async function refresh(): Promise<void> {
    setInstalled(await listInstalled());
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function onUninstall(id: string): Promise<void> {
    await uninstall(id);
    await reinstallAllPolicies();
    await refresh();
  }

  if (installed.length === 0) {
    return <p>No bundles installed yet. Visit the Marketplace tab.</p>;
  }
  return (
    <ul className="installed">
      {installed.map((b) => (
        <li key={b.bundle_id}>
          <h3>{b.bundle_id}</h3>
          <small>v{b.version} • installed {new Date(b.installedAtMs).toLocaleString()}</small>
          <details>
            <summary>{b.renderedPolicySet.length} policies</summary>
            <ul>
              {b.renderedPolicySet.map((p) => (
                <li key={p.id} className="policy">{p.id}</li>
              ))}
            </ul>
          </details>
          <button onClick={() => void onUninstall(b.bundle_id)}>Uninstall</button>
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 6: styles.css** (minimal)

```css
:root {
  font-family: ui-sans-serif, system-ui, sans-serif;
  color-scheme: light dark;
  background: #0c0c10;
  color: #e9e9ee;
}
body { margin: 0; }
.root { max-width: 720px; margin: 0 auto; padding: 24px; }
nav { display: flex; gap: 8px; margin-bottom: 24px; }
nav button { padding: 8px 16px; background: #16161d; color: inherit; border: 1px solid #2c2c34; border-radius: 6px; cursor: pointer; }
nav button.active { background: #2c2c34; }
.bundles, .installed { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 16px; }
.bundles li, .installed li { padding: 16px; background: #16161d; border-radius: 8px; }
.bundles h3, .installed h3 { margin: 0 0 4px 0; font-size: 16px; }
.bundles small, .installed small { color: #888; font-size: 12px; }
.bundles button, .installed button { margin-top: 12px; padding: 6px 12px; background: #f1c40f; color: #0c0c10; border: 0; border-radius: 4px; cursor: pointer; font-weight: 600; }
.policy { font-family: ui-monospace, monospace; font-size: 12px; color: #aaa; }
.error { color: #e74c3c; }
```

- [ ] **Step 7: Build + commit**

```bash
cd extension && yarn build:chrome 2>&1 | tail -5
git add extension/public/options.html extension/src/marketplace/ extension/src/manifest.json extension/webpack/
git commit -m "feat(extension): marketplace + installed-bundles UI"
```

---

## Task 9: End-to-end installer test with real WASM

**Files:** Create `extension/src/background/marketplace/__tests__/installer.test.ts`. Requires the wasm-pack artifact from Plan 2 to be present in `extension/public/wasm/`.

- [ ] **Step 1: Build WASM**

```bash
cd .. && wasm-pack build crates/policy_engine_wasm --target web --release --out-dir ../extension/public/wasm
cd extension && yarn build:chrome
```

- [ ] **Step 2: Write the test**

```typescript
import { describe, expect, it, vi, beforeAll } from 'vitest';
import JSZip from 'jszip';
import { sha256 } from '@noble/hashes/sha256';
import { ed25519 } from '@noble/curves/ed25519';

const sk = ed25519.utils.randomPrivateKey();
const pk = Buffer.from(ed25519.getPublicKey(sk)).toString('hex');
vi.stubEnv('SCOPEBALL_PUBLISHER_PUBKEY', pk);
vi.stubEnv('SCOPEBALL_CATALOG_URL', 'https://catalog.example/index.json');

// Real WASM bridge load. The dynamic-import path uses Browser.runtime.getURL,
// which does not work in vitest — so we stub it to file:// URLs against the
// built artifact.
vi.mock('webextension-polyfill', () => ({
  default: {
    runtime: {
      getURL: (s: string) => `file://${process.cwd()}/public/${s}`,
    },
    storage: {
      local: {
        get: vi.fn(async () => ({})),
        set: vi.fn(async () => {}),
        remove: vi.fn(async () => {}),
      },
      session: {
        get: vi.fn(async () => ({})),
        set: vi.fn(async () => {}),
      },
    },
    windows: { create: vi.fn(), remove: vi.fn(), onRemoved: { addListener: vi.fn() } },
    alarms: { create: vi.fn(), onAlarm: { addListener: vi.fn() } },
  },
}));

import { installBundle } from '../installer';
import type { CatalogBundleEntry } from '../catalog-client';

async function buildBundleZip(): Promise<{ bytes: Uint8Array; sha256Hex: string }> {
  const zip = new JSZip();
  zip.file(
    'manifest.json',
    JSON.stringify({
      manifest_version: 1,
      bundle_id: 'demo-cap',
      version: '1.0.0',
      author: { name: 'demo', pubkey: pk },
      policies: [{ id: 'cap', file: 'policies/cap.cedar.tmpl', default_severity: 'deny' }],
      params_schema: 'params.schema.json',
    }),
  );
  zip.file('params.schema.json', JSON.stringify({ cap: { type: 'integer', min: 1, max: 10000 } }));
  zip.file(
    'policies/cap.cedar.tmpl',
    'permit(principal, action, resource) when { context.totalInputUsd <= {{cap}} };',
  );
  const bytes = await zip.generateAsync({ type: 'uint8array' });
  const hex = Buffer.from(sha256(bytes)).toString('hex');
  return { bytes, sha256Hex: hex };
}

describe('installer end-to-end', () => {
  let bundle: { bytes: Uint8Array; sha256Hex: string };

  beforeAll(async () => {
    bundle = await buildBundleZip();
  });

  it('rejects an injection-attempt template at AST-equivalence step', async () => {
    const evilZip = new JSZip();
    evilZip.file(
      'manifest.json',
      JSON.stringify({
        manifest_version: 1,
        bundle_id: 'evil',
        version: '1.0.0',
        author: { name: 'evil', pubkey: pk },
        policies: [{ id: 'cap', file: 'policies/cap.cedar.tmpl', default_severity: 'deny' }],
        params_schema: 'params.schema.json',
      }),
    );
    evilZip.file('params.schema.json', JSON.stringify({ cap: { type: 'integer', min: 0, max: 1 } }));
    evilZip.file(
      'policies/cap.cedar.tmpl',
      // {{cap}} occupies the entire predicate. Layer-0 (assertSlotPositions)
      // rejects this regardless of integer-blanker behavior — the slot's
      // preceding token is `{ `, not a comparison op or `in` keyword.
      'permit(principal, action, resource) when { {{cap}} };',
    );
    const evilBytes = await evilZip.generateAsync({ type: 'uint8array' });
    const evilSha = Buffer.from(sha256(evilBytes)).toString('hex');

    const evilEntry: CatalogBundleEntry = {
      bundle_id: 'evil',
      version: '1.0.0',
      manifest_url: 'data:,',
      bundle_url: 'https://catalog.example/evil.zip',
      bundle_sha256: evilSha,
      author: { name: 'evil', pubkey: pk },
    };

    // We bypass catalog fetch and call fetchBundle directly via stub.
    const fetchMock = vi.fn(async () => new Response(evilBytes));
    vi.stubGlobal('fetch', fetchMock);

    // Layer-0 slot-position whitelist (introduced in fix-pass) rejects
    // predicate-as-slot templates up-front. installBundle propagates the
    // rejection from renderAndVerify.
    await expect(installBundle(evilEntry, { cap: 1 })).rejects.toThrow(/non-whitelisted/);
  });
});
```

> The test above intentionally documents the boundary case: AST equivalence catches structural mutations but not pure-literal substitution that's still semantically dangerous. Defense-in-depth comes from layers 1+2 (param validator + Cedar literal serializer's type constraints) — a `boolean` param type would prevent this exact attack but is out of v1 scope.

- [ ] **Step 3: Run + commit**

```bash
cd extension && yarn test installer 2>&1 | tail -15
git add extension/src/background/marketplace/__tests__/installer.test.ts
git commit -m "test(extension): installer end-to-end (sandbox + render + AST check)"
```

---

## Task 10: Author tooling — sign + publish a bundle

**Files:** Create `extension/scripts/sign-bundle.js`, `extension/scripts/sign-catalog.js`. These run *outside* the extension at bundle-author time; they're checked in for reproducibility.

- [ ] **Step 1: sign-bundle.js**

```javascript
#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const { sha256 } = require('@noble/hashes/sha256');
const JSZip = require('jszip');

// Deterministic timestamp for zip entries — re-running sign-bundle.js on the
// same input must produce a byte-identical zip (and therefore identical
// sha256). Without this, JSZip uses Date.now() per entry → hash drifts.
const FIXED_DATE = new Date('2026-01-01T00:00:00Z');

async function main() {
  const [, , bundleDir] = process.argv;
  if (!bundleDir) {
    console.error('usage: sign-bundle.js <bundle-dir>');
    process.exit(2);
  }
  const zip = new JSZip();
  function addFile(rel) {
    zip.file(rel, fs.readFileSync(path.join(bundleDir, rel)), {
      date: FIXED_DATE,
      unixPermissions: 0o644,
    });
  }

  // Sorted file list — readdirSync order is filesystem-dependent. Sorting
  // makes the zip's central directory identical across machines.
  const files = ['manifest.json', 'params.schema.json'];
  const policiesDir = path.join(bundleDir, 'policies');
  const policyFiles = fs.readdirSync(policiesDir).sort();
  for (const f of policyFiles) files.push(path.join('policies', f));
  files.sort();
  for (const f of files) addFile(f);

  const bytes = await zip.generateAsync({
    type: 'uint8array',
    compression: 'STORE', // no compression → no compressor-version drift
    streamFiles: false,
  });
  const sha = Buffer.from(sha256(bytes)).toString('hex');
  const outZip = `${bundleDir}.zip`;
  fs.writeFileSync(outZip, bytes);
  console.log(`bundle: ${outZip}`);
  console.log(`sha256: ${sha}`);
}

main();
```

- [ ] **Step 2: sign-catalog.js**

```javascript
#!/usr/bin/env node
const fs = require('fs');
const { ed25519 } = require('@noble/curves/ed25519');
const canonicalize = require('canonicalize');

const [, , inputPath, skPath, outPath] = process.argv;
if (!inputPath || !skPath || !outPath) {
  console.error('usage: sign-catalog.js <unsigned-index.json> <publisher.sk> <out.json>');
  process.exit(2);
}
const sk = Uint8Array.from(Buffer.from(fs.readFileSync(skPath, 'utf8').trim(), 'hex'));
const unsigned = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
const pkHex = Buffer.from(ed25519.getPublicKey(sk)).toString('hex');
const rest = { ...unsigned, publisher_pubkey: pkHex };
// RFC 8785 (JCS): byte-identical to catalog-client.ts's verify path.
const canonical = canonicalize(rest);
if (canonical === undefined) {
  console.error('catalog body did not canonicalize');
  process.exit(3);
}
const sig = ed25519.sign(new TextEncoder().encode(canonical), sk);
const signed = { ...rest, signature: Buffer.from(sig).toString('hex') };
fs.writeFileSync(outPath, JSON.stringify(signed, null, 2));
console.log(`signed catalog written to ${outPath}`);
console.log(`publisher pubkey: ${pkHex}`);
```

- [ ] **Step 3: Document**

Append to `extension/README.md`:

```markdown
## Authoring + publishing a policy bundle

1. Create a bundle directory:

   ```
   my-bundle/
   ├── manifest.json
   ├── params.schema.json
   └── policies/
       └── max-input-usd.cedar.tmpl
   ```

2. Sign the bundle (produces zip + sha256):

   ```sh
   node extension/scripts/sign-bundle.js my-bundle
   ```

3. Add an entry to your unsigned catalog index, then sign:

   ```sh
   node extension/scripts/sign-catalog.js unsigned-index.json publisher.sk index.json
   ```

4. Publish `index.json` and the bundle zip on GitHub Pages or any static
   HTTPS host. Pin the publisher pubkey in the extension's
   `SCOPEBALL_PUBLISHER_PUBKEY` build env.
```

- [ ] **Step 4: Commit**

```bash
git add extension/scripts/sign-bundle.js extension/scripts/sign-catalog.js extension/README.md
git commit -m "docs(extension): bundle + catalog authoring scripts"
```

---

## Self-review summary

**Spec coverage** (vs design §5, §6):
- ✅ Static catalog fetch + Ed25519 signature verification — Task 3
- ✅ Bundle layout + sha256 integrity — Task 4
- ✅ Sandbox enforcement (`.cedar.tmpl` + manifest + params only) — Task 4
- ✅ Typed parameter schema — Task 5
- ✅ Cedar literal serializer per type — Task 5
- ✅ AST equivalence guard against structural injection — Task 1, 6
- ✅ First-install pubkey pinning + bundle_id namespacing — Task 7
- ✅ Marketplace + Settings UI — Task 8
- ✅ Author tooling (sign-bundle, sign-catalog) — Task 10
- ⏭ Manifest version 2 (schema_extensions), publisher key rotation, full per-bundle param editing UI → v1.1+

**Risks flagged for the executor:**
- The `cedar-policy::Policy::to_json()` AST shape may differ from this plan's literal-key list — verify and adjust at Task 1 step 1
- The AST equivalence test in Task 9 documents a known limitation: pure-literal injection inside a fully-templated predicate is *structurally* valid; it's blocked by Layer-1 type constraints (no `boolean` param type, integers only at numeric positions), not by Layer-3 AST. Surfacing this in the test prevents future silent degradation
- WASM bundle size grows with the `parse_policy_ast_json` export (cedar-policy's full AST serialization brings weight) — recheck the §3.2 latency budget after Plan 6 ships
- The `installBundle` flow does not validate that the bundle's referenced Cedar context fields (`context.totalInputUsd`, etc.) match the engine's actual schema. A bundle that references undeclared fields will fail at `install_policies_json` time — that's correct fail-closed behavior, but the UI should surface a friendlier error (UI follow-up)
