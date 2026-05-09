# Per-Policy Selection Popup — Design

**Status:** approved (2026-05-09)
**Author:** Woojin Lee
**Audience:** engine + extension contributors

## 1. Problem

The Scopeball Chrome extension currently installs all 20 default policies
(plus every marketplace bundle's rendered set) on service-worker boot. For
testers verifying behavior policy-by-policy, this is unworkable: a single
transaction can match multiple policies, so it is impossible to attribute
a verdict to one rule in isolation.

**User quote:**
> Extension에서 유저가 직접 사용할 policy를 설정할 수 있는 창을 하나 만들 수 있어?
> 한번 테스트를 해보고 싶은데, 테스트용 policy들이 너무 많아서 policy별로 검증을 할
> 수가 없네. 유저(테스터)가 사용할 policy를 설정하면, 그 설정해둔 policy들만 가지고
> transaction을 필터링하면 좋겠어.

## 2. Goals / Non-goals

**Goals (v1)**

- Provide a popup UI listing every known policy (default + every installed
  marketplace bundle), each with an on/off toggle.
- Persist the user's selection across browser restarts.
- Apply selection by reinstalling only the enabled policies into the WASM
  engine — disabled policies must not contribute to verdicts.
- Default state on first install: **all policies disabled** (explicit opt-in
  per tester request).

**Non-goals**

- Per-actor or per-chain selection.
- Severity-level filtering ("show only fail-severity").
- Import/export of selection JSON or selection-sync via `chrome.storage.sync`.
- A separate options page; the toolbar popup is sufficient.

## 3. Architecture

```
popup.html / popup/index.ts          (vanilla TS, mirrors confirm/)
        │  message: 'policy-catalog' | 'set-enabled-ids'
        ▼
background/policy-selection.ts       (new — selection store + apply queue)
        │  read/write 'policy-selection:enabled-ids'
        ▼
chrome.storage.local                 + 'policy-selection:applied-ids'
        │  consumed by
        ▼
background/policies-loader.ts        (modified — filters before install)
        │
        ▼
WASM engine: install_policies_json   (only enabled policies installed)
```

State separation: `enabled-ids` (desired, written by popup) vs
`applied-ids` (active in WASM, written by loader after a successful
install). Mismatch between the two is the popup's "Reinstalling…" state.

Filtering happens at install time, not per decision. The engine API stays
unchanged. This also benefits marketplace bundles for free, since their
policies flow through the same `installPolicies()` call.

**Apply ordering & concurrency.** Toggle handling MUST be serialized on
the background side: a single in-flight reinstall plus a tail "next
desired" slot. Concurrent or rapid toggles collapse to one queued apply,
not a race over `installed`/`inflight` in the loader. On reinstall
failure, the loader clears `inflight`/`installed` so the next call
retries cleanly (current `policies-loader.ts:31–42` poisons `inflight`
on rejection — this is fixed as part of v1).

## 4. Components

### 4.1 popup/index.ts (new)

Vanilla TS module rendering the popup body. Pattern matches
`extension/src/confirm/index.ts` (no React dependency). Adds a new
webpack entry `popup/index` to `extension/webpack/webpack.common.js`
(currently 7 entries: background, three content-scripts, injected,
confirm/index, manifest — popup/index makes 8).

Renders, inside a fixed-width (~360px) body with sticky header/footer
and a scrollable middle:

- **Header** — `N of M enabled`, plus `[Enable all]` `[Disable all]`. If
  `M > 0` and `N === 0`, also a banner: *"All policies disabled — every
  Cedar verdict will pass; the orchestrator may still warn on
  unsupported request paths."*
- **Search** — filters rows by id substring or reason text.
- **Sections grouped by source.** Defaults grouped by namespace prefix
  (`default::dex`, `default::signature/_shared`, …). Each installed
  marketplace bundle gets its own section labeled by `bundle_id +
  version`.
- **Row per policy_set entry** (NOT per Cedar `@id` clause). Some shared
  signature entries contain 2–3 `forbid` clauses; the row shows the
  entry id once, the dominant severity (highest of `deny > warn`), and
  if multiple distinct reasons are present, the first reason with a
  `+N more` chip.
- **Per-row "Only this" button** — disables every other policy and
  enables this one. Required for fast policy-by-policy testing.
- **Footer** — status line: `Reinstalling…` / `Up to date` /
  `Error: <kind> <message>`.

Apply behavior: every toggle posts `set-enabled-ids` immediately (no
client-side debounce, since popup unmount can drop pending timers). The
background queue collapses adjacent applies.

### 4.2 lib/policy-meta.ts (new)

Pure-TS parser pulling `@id`, `@severity`, `@reason` annotations out of a
Cedar policy text. Lives in `src/lib/` because both the popup and the
background `getCatalog()` import it. Output:

```ts
{ shortId: string;
  rules: { severity: 'deny' | 'warn'; reason: string }[];
  dominantSeverity: 'deny' | 'warn' | 'unknown'; }
```

`severity` values match what the engine emits
(`crates/policy-engine/src/policy.rs:104–112` — `deny | warn`); a
missing `@severity` annotation falls through to `unknown`. Tested in
isolation; no DOM, no chrome.* dependency.

### 4.3 background/policy-selection.ts (new)

Selection store + apply queue. Public surface:

```ts
export async function getEnabledIds(): Promise<string[]>;
export async function getAppliedIds(): Promise<string[]>;
export async function applyEnabledIds(ids: string[]): Promise<{ok: true} | {ok: false; error: {kind: string; message: string}}>;
export async function getCatalog(): Promise<{
  policies: { id: string; rules: { severity: string; reason: string }[]; dominantSeverity: string; sourceLabel: string }[];
  enabled: string[];
  applied: string[];
}>;
```

`applyEnabledIds()` writes `enabled-ids`, then runs through a single
serialization queue: at most one in-flight reinstall, with a "next
desired" slot — newer requests overwrite older queued ones. After the
WASM `install_policies_json` returns ok, the loader writes `applied-ids
= enabled-ids` and the call resolves. If install fails, `applied-ids`
stays at the previous value and the error is surfaced.

`getCatalog()` recomputes on each call: it loads the bundled defaults
and iterates `listInstalled()` (NOT `aggregatedPolicySet()`, which
flattens away `bundle_id`) to build per-bundle sections. It also
filters stale ids out of `enabled-ids` before returning the count, so
uninstalled marketplace bundles don't inflate `<N of M>`.

### 4.4 background/policies-loader.ts (modified)

`ensureDefaultPoliciesInstalled()` and `reinstallAllPolicies()` both
intersect the union of (defaults ∪ marketplace) with the enabled-id set
before calling `installPolicies()`. If the enabled set is empty, the call
becomes `installPolicies({ schema_text, policy_set: [] })` — valid;
builder always injects an `engine/baseline-allow` rule
(`crates/policy-engine/src/policy.rs:549`).

Bug fix folded in: today, an install rejection leaves
`inflight`/`installed` poisoned (the rejected promise is cached). The
modified loader clears both on rejection so the next call retries.

### 4.5 background/index.ts (modified)

Adds two `runtime.onMessage` handlers (orthogonal to the existing
content-script port):

- `{type:'policy-catalog'}` → returns `getCatalog()`.
- `{type:'set-enabled-ids', ids:string[]}` → calls
  `applyEnabledIds(ids)` and replies with its result.

## 5. Data flow

1. User clicks toolbar icon → `popup.html` opens → `popup/index.ts` runs.
2. Popup posts `policy-catalog` → background returns
   `{policies, enabled, applied}`.
3. Popup renders rows. A row shows its `enabled` checkbox; the footer
   shows `Up to date` if `enabled === applied`, else `Reinstalling…`.
4. User toggles a row (or clicks "Only this") → popup posts
   `set-enabled-ids` immediately with the new id list.
5. Background's apply queue runs one reinstall at a time; a newer
   `set-enabled-ids` arriving mid-flight overwrites the queued tail.
6. After `install_policies_json` returns ok, background writes
   `applied-ids` and replies `{ok:true}`. Popup footer flips to
   `Up to date`.

## 6. Error handling

- **Reinstall failure** (e.g. malformed marketplace policy text):
  background reports `{ok:false, error:{kind, message}}`; popup footer
  shows `Error: <kind> <message>`. `applied-ids` is NOT updated, so the
  popup shows the still-active set as the source of truth and can offer
  a "Retry" action.
- **Loader-state poison**: the modified `policies-loader.ts` clears
  `installed` and `inflight` on rejection so a retry actually retries
  rather than re-throwing the cached error.
- **Storage write failure**: popup reverts the toggle visually and
  surfaces the thrown message.
- **Empty enabled set**: a valid install. The Cedar engine evaluates
  with only its baseline-allow rule, so policy verdicts come back
  `pass`. The orchestrator can still emit `warn`/`fail` for unsupported
  paths *before* policy evaluation
  (`extension/src/background/orchestrator.ts:188–199`); the popup
  banner makes that distinction explicit.
- **Catalog parse failure** (no `@severity` / `@reason` in a
  marketplace policy): row falls back to
  `dominantSeverity:'unknown'`, `reason:'(no reason annotation)'`.
  Never hides the row.
- **Stale `enabled-ids`** (an id no longer present in defaults or any
  installed bundle): `getCatalog()` filters them out of the count and
  the next `applyEnabledIds()` write trims them from storage.

## 7. Testing

- **Unit (lib/policy-meta.ts):** parse Cedar texts including
  multi-rule entries and entries missing `@severity` / `@reason`. Verify
  `dominantSeverity` is the highest of `deny > warn > unknown`.
- **Unit (background/policy-selection.ts):**
  - roundtrip set/get against an in-memory `chrome.storage.local` mock;
  - apply queue collapses three rapid `applyEnabledIds()` calls into
    one in-flight + one tail;
  - install rejection leaves `applied-ids` unchanged but does NOT
    poison subsequent calls;
  - `getCatalog()` filters out stale ids from the count.
- **Unit (policies-loader.ts):** with stub `installPolicies`, assert
  enabling 2 of 20 ids produces `policy_set.length === 2`. Empty
  enabled set produces `policy_set.length === 0`.
- **Integration (real WASM):** enable only
  `default::dex/max-input-usd-100`, fire a request that would otherwise
  trip `default::dex/uniswap-only-allowlist`; assert the disabled
  policy id is not in the matched list.
- **Integration (concurrency):** issue two `set-enabled-ids` calls back
  to back; assert exactly one extra reinstall ran and `applied-ids`
  matches the second call.
- **Manual e2e:** load unpacked, open popup, click "Only this" on one
  policy, trigger a dApp tx, confirm the verdict modal cites only that
  policy.

## 8. Out of scope / future

- Selection presets ("All DEX policies", "All Signature policies"): the
  per-row "Only this" plus per-section "Enable all in section" covers
  v1's testing needs without adding a preset registry.
- Severity-only filters.
- Per-bundle "block all from this author".
- Selection sync across devices via `chrome.storage.sync`.
- Bulk diff against a recommended baseline.
- A "Reset" button — there is no precise baseline to reset *to* (first
  run is "all disabled", which `[Disable all]` already covers).
- Cleaning up the existing schema-passthrough quirk: `policies-loader.ts`
  passes the bundled `schema.cedarschema` text to
  `install_policies_json`, while the WASM builder also preloads bundled
  schema (`crates/policy-engine/src/policy.rs:496–509`). Today this works
  because the on-disk file does not redeclare bundled entities, but it
  is brittle. Tracked separately; this spec preserves current behavior.

## 9. File touch list

```
extension/public/popup.html                       modify  (mirror confirm.html: <main id="root"> + <script src="js/popup/index.js">)
extension/src/popup/index.ts                      create
extension/src/popup/styles.css                    create  (imported via style-loader from index.ts)
extension/src/lib/policy-meta.ts                  create  (shared by popup + background)
extension/src/lib/policy-meta.test.ts             create
extension/src/background/policy-selection.ts     create  (selection store + apply queue + getCatalog)
extension/src/background/policy-selection.test.ts create  (queue collapse, stale-id filter, rollback)
extension/src/background/policies-loader.ts      modify  (filter by enabled-ids, clear inflight on reject)
extension/src/background/policies-loader.test.ts create  (filter correctness, empty-set install)
extension/src/background/index.ts                modify  (onMessage handlers for catalog / set-enabled-ids)
extension/webpack/webpack.common.js              modify  (add popup/index entry — 8th)
extension/src/manifest.json                      no-op   (popup already registered)
```
