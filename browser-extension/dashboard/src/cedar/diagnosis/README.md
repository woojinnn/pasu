# Denial diagnosis (`cedar/diagnosis`)

**What it does:** when a Cedar policy *denies* a transaction, this module pinpoints the
exact sub-clause(s) that caused the denial so a surface can highlight them — a red box on
the offending blocks in the editor, or a culprit line in the confirm popup.

This README is the **front-door for frontend developers**. If you only read one file, read
this one; it tells you which functions to call, in what order, and the one mistake that
silently breaks everything.

---

## 1. The mental model (read this first)

The policy engine is **default-allow**: there is a hidden baseline `permit`, and every
user-authored rule is a `forbid`. So:

> **A denial means a `forbid` *fired*** — its `when` conditions evaluated **TRUE**
> (and its `unless` conditions **FALSE**).

The clause you want to red-box is therefore the **satisfied** condition that caused the
block, **never** a "false" node. Example: `forbid … when { slippageBp > 100 }` blocking a
150-bp swap → the culprit is "`slippageBp (150) > 100` was **true**".

We never re-implement Cedar's evaluator. Instead we use **Cedar itself as an oracle**:

1. For every boolean node of the denying policy, build a tiny probe
   `permit(...) when { <that subtree> }`, tagged with the node's structural path as its
   `@id` (e.g. `c0.body.left`).
2. Run Cedar's `Authorizer` **once** over all probes, against the **same materialized
   context** the real verdict used.
3. Read `reason()` → the probe ids whose body was **true**; `errors()` → ids whose body
   **errored**. That gives a **truth map** `{ path → true | false | error }`.
4. A pure-TS **blame walker** turns that truth map into the responsible *leaf* paths using
   an AND/OR/NOT/if rule (e.g. `&&` true ⇒ both sides are the cause; `||` true ⇒ only the
   true side is).

Because each probe body is a *verbatim* sub-expression of the real policy, evaluated by
*the same Cedar* against *the same context*, the truth values **cannot disagree** with the
real verdict. That is what makes the attribution provable — there is no second evaluation
semantics to keep in sync.

---

## 2. Public API — what you call

Exported from the modules noted in each row — most from the `cedar/diagnosis` barrel
(`index.ts`); `pathToBlockId` from `cedar/diagnosis/path`; `applyCulprits` / `clearCulprits`
from `editor-v9/diagnosis-highlight`:

| Function | Where it runs | What it does |
| --- | --- | --- |
| `buildProbes(policy)` → `{ probes, diagnosable }` | TS | Enumerate the policy's boolean nodes and build one probe each. `diagnosable` is `false` if the policy contains a `hole`/`raw` node (then fall back to the static `@reason`). |
| `runDiagnosisProbes(request)` → `Promise<{ true_ids, error_ids }>` | bridge → WASM | Send the action + probes to the WASM oracle (via the service worker) and get the truth-map id sets back. From `server-api/diagnosis`. |
| `diagnoseFromResult(policy, probeIds, result)` → `{ culprits, errored }` | TS | Turn the WASM result into the responsible leaf **paths** (`culprits`) via the blame walker. `errored` paths are surfaced separately, never as culprits. |
| `pathToBlockId(policy, blockIdByNode)` → `Map<path, blockId>` | TS | Combine the policy's node paths with an `Expr→blockId` identity map (recorded by `irToWorkspace`) so you can map a culprit path to a Blockly block. From `cedar/diagnosis/path`. |
| `applyCulprits(ws, pathMap, paths, note?)` / `clearCulprits(ws)` | TS (Blockly) | Red-box the culprit blocks (and optionally annotate them). From `editor-v9/diagnosis-highlight`. |

You normally **do not** call `blame(...)` directly — `diagnoseFromResult` wraps it and
also handles the false-inclusive truth map and the errored-path filtering for you.

---

## 3. End-to-end recipe — add diagnosis to a surface

This is the canonical flow. The shipped reference implementation is the editor's
**Simulate** button: `editor-v9/Workspace.tsx → onSimulate` (copy that pattern).

```ts
import { buildProbes, diagnoseFromResult } from "../cedar/diagnosis";
import { pathToBlockId, enumeratePaths } from "../cedar/diagnosis/path";
import { runDiagnosisProbes } from "../server-api/diagnosis";
import { applyCulprits, clearCulprits } from "./diagnosis-highlight";
import { SAMPLE_ACTIONS } from "./sample-actions";

// `policies` = workspaceToIR(ws) — call it ONCE. `policy` = the forbid you diagnose
//   (e.g. policies[0]); pass the SAME objects everywhere (object identity matters — §4).
// `sample` = a factory `() => { action, meta, tx, bundles, results }` describing the
//   transaction to evaluate against (e.g. `SAMPLE_ACTIONS[actionId]`).

// 0. Diagnosis only makes sense for `forbid` policies (a fired forbid = denial).
if (policy.effect !== "forbid") return;

// 1. Build the probes (one per boolean node). Bail out to @reason if not diagnosable.
const { probes, diagnosable } = buildProbes(policy);
if (!diagnosable) {
  // policy has a hole/raw node → show the static @reason instead of a wrong box.
  return;
}

// 2. (editor only) Re-render the workspace so the Expr→blockId identity map is keyed
//    by the SAME PolicyIR objects we are about to diagnose. SEE THE GOTCHA in §4.
const blockIdByNode = new Map<Expr, string>();
irToWorkspace(ws, policies, blockIdByNode); // pass the SAME `policies` you diagnose

// 3. Run the Cedar oracle (crosses to WASM via the service worker).
const result = await runDiagnosisProbes({ ...sample(), probes }); // sample is a factory — call it

// 4. Resolve the responsible leaf paths.
const { culprits, errored } = diagnoseFromResult(policy, probes.map((p) => p.id), result);

// 5. Map paths → block ids and highlight.
const pathMap = pathToBlockId(policy, blockIdByNode);
applyCulprits(ws, pathMap, culprits /*, optional note(path) => string */);
```

For a **non-editor** surface (e.g. the confirm popup) you skip steps 2 and 5 — you have no
Blockly workspace — and instead render `culprits` as text. Build a `path → Expr` map from
`enumeratePaths(policy)` to resolve each full culprit path to its `Expr` (`nodeAtPath` only
resolves a single step), then gloss it into a line such as `"차단: slippageBp (150) > 100"`.

---

## 4. ⚠️ The one gotcha — the identity-map seam

`pathToBlockId` is keyed by **object identity**, not by value. The `Expr` objects that
`irToWorkspace` recorded into `blockIdByNode` must be the **exact same objects** you pass to
`buildProbes` / `diagnoseFromResult` / `pathToBlockId`.

`workspaceToIR()` returns **fresh** `PolicyIR` objects each call. So if you build probes from
one `workspaceToIR()` result but recorded the identity map from a different render, every
path resolves to nothing and **zero blocks highlight** — a silent failure with no error.

The fix (what `onSimulate` does): call `workspaceToIR()` **once**, then pass that *same*
`policy`/`policies` object to *both* `irToWorkspace(ws, policies, map)` *and* the diagnosis
calls. No intervening edit, so identity holds by construction.

---

## 5. Data shapes

```ts
// Sent to the WASM oracle (server-api/diagnosis.ts):
interface DiagnosisRequestDto {
  action: unknown;   // ActionBody — the action being evaluated
  meta:   unknown;   // ActionMeta
  tx:     { chain_id: string; from: string; to: string };
  bundles: { policy: string; manifest: unknown }[];
  results: Record<string, unknown>;  // host RPC results (empty for base-context sims)
  probes:  { id: string; est: unknown }[];  // from buildProbes()
}

// Returned by the oracle:
interface DiagnosisResultDto {
  true_ids:  string[];  // probe ids (= node paths) whose body was TRUE
  error_ids: string[];  // probe ids whose body ERRORED (e.g. missing attr)
}

// Returned by diagnoseFromResult():
interface Diagnosis {
  culprits: string[];  // responsible leaf paths — highlight these
  errored:  string[];  // paths whose probe errored — render a distinct "uneval" state
}
```

A path absent from `true_ids` is treated as **false** (the truth map is false-inclusive),
which is what makes the `unless` / "false-side" blame branches work.

---

## 6. Node-path scheme

A node has no id, so it is addressed by its structural path from the policy root. The labels
are defined in **one** place — `path.ts::eachChild` — and every consumer derives paths from
it, so they cannot drift apart:

- `c{i}.body` → `conditions[i].body`
- then per `Expr` kind: `.left` / `.right` (binary) · `.operand` (unary) · `.of`
  (attr/has/like/is) · `.in` (is only, when an `in` clause is present) · `.cond` / `.then` /
  `.else` (if) · `.elements[k]` (set) · `.pairs[k]` (record value) · `.args[k]` (ext)

---

## 7. File map

| File | Role |
| --- | --- |
| `path.ts` | Node-path scheme. **Sole owner** of step labels (`eachChild`). Exposes `enumeratePaths`, `pathByNode`, `pathToBlockId`, `nodeAtPath`. |
| `probes.ts` | `buildProbes` — boolean-node enumeration + probe (EST) builder. |
| `blame.ts` | `blame` — the AND/OR/NOT/if polarity-aware truth-map → culprit-leaf walker. |
| `index.ts` | Public barrel + `diagnoseFromResult` orchestrator + the `Diagnosis` type. |
| `../../server-api/diagnosis.ts` | `runDiagnosisProbes` — the bridge call into the WASM oracle. |
| `../../editor-v9/diagnosis-highlight.ts` | `applyCulprits` / `clearCulprits` — Blockly red-box rendering. |
| `../../editor-v9/sample-actions.ts` | `SAMPLE_ACTIONS` — built-in sample actions to simulate against. |
| `../../editor-v9/Workspace.tsx` (`onSimulate`) | **Reference integration** — copy this. |

The runner lives on the backend: WASM `run_diagnosis_probes_v2_json`
(`crates/policy-engine-wasm/src/diagnosis_exports.rs`), reached via the service-worker op
`run-diagnosis-probes` (`backend/service-worker/index.ts`) and the bridge wrapper
`runDiagnosisProbesV2` (`backend/service-worker/wasm-bridge.ts`).

---

## 8. Scope & limits (v1)

- **Editor "Simulate" only.** The confirm-popup culprit line is a planned follow-on (it
  crosses the backend/dashboard package boundary; see the design doc's Phase 5).
- **`forbid` policies only.** A `permit` policy would invert the probe polarity, so
  Simulate is gated to `forbid`.
- **Single action.** Multicall per-child diagnosis is a later extension.
- **`hole`/`raw` policies are not diagnosable** → `diagnosable === false`, fall back to the
  static `@reason`.
- **Base-context sims** in the editor (no host RPC results required); the popup path reuses
  the results it already fetched.
