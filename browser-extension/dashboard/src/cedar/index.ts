/**
 * Cedar bridge — page → extension service worker.
 *
 * Routes Cedar validate / test / simulate calls through the
 * `dashboard-bridge` content script (manifest matches localhost:5173-5)
 * to the SW's `cedar-validate` / `cedar-test` / `cedar-simulate`
 * handlers, which in turn call into `policy-engine-wasm`.
 *
 * If the extension isn't installed / the content script isn't injected,
 * `sendToExtension` rejects with `ExtensionBridgeTimeout` after a short
 * deadline. The UI catches that and shows a soft "wasm 미연결" hint
 * (see CodeEditor + EditorWorkspace drawer status).
 */

import {
  ExtensionBridgeTimeout,
  sendToExtension,
} from "../server-api/extension-bridge";
import type { PolicySeverity, Verdict } from "../server-api";
import { blocksToEst, estToBlocks } from "./blocks";
import type { PolicyIR, SchemaDescriptor } from "./blocks";

// ── public types (match the old api-client shapes) ──────────────────────

export interface ValidateResp {
  /** Best-guess validity. When `skipped === true`, this defaults to `true`
   *  (don't gate save on un-runnable validation). */
  ok: boolean;
  /** `true` when no real validator was available — UI should show a soft
   *  "검증 건너뜀" hint instead of a red error. */
  skipped?: boolean;
  error?: string;
}

export interface CedarRequestInput {
  principal: string;
  action: string;
  resource: string;
  entities?: unknown[];
  context?: Record<string, unknown>;
}

export interface MatchedPolicyDto {
  policy_id: string;
  severity: "deny" | "warn";
  reason?: string;
}

export interface TestPolicyResp {
  verdict: Verdict;
  matched: MatchedPolicyDto[];
}

export interface PolicyInput {
  policy_id: number;
  policy_name: string;
  severity: PolicySeverity;
  cedar_text: string;
}

export interface SequenceStepInput {
  label?: string;
  principal: string;
  action: string;
  resource: string;
  entities?: unknown[];
  context?: Record<string, unknown>;
}

export interface PolicyOutcome {
  policy_id: number;
  policy_name: string;
  severity: PolicySeverity;
  decision: "allow" | "deny";
  matched?: string[];
}

export interface SequenceStepResult {
  label: string | null;
  verdict: Verdict;
  policy_results: PolicyOutcome[];
}

export interface SequenceResp {
  overall: Verdict;
  steps: SequenceStepResult[];
}

// ── helpers ─────────────────────────────────────────────────────────────

/** Short timeout — wasm calls return in <50ms locally. A long timeout
 *  hides "extension not installed" issues; keep it tight. */
const BRIDGE_TIMEOUT_MS = 2_000;

/** When the dashboard runs without the extension installed, the bridge
 *  times out. Callers want a soft "skipped" result instead of an error
 *  so the UI doesn't gate save on something we can't run. */
function isMissingBridge(err: unknown): boolean {
  return err instanceof ExtensionBridgeTimeout;
}

// ── public api ─────────────────────────────────────────────────────────

/** Idempotent wasm init. The SW lazy-loads wasm on first cedar message,
 *  so a single ping is enough to warm the cache. */
export async function ensureCedarReady(): Promise<void> {
  try {
    await sendToExtension({ type: "cedar-validate", text: "permit(principal, action, resource);" }, BRIDGE_TIMEOUT_MS);
  } catch (err) {
    if (isMissingBridge(err)) return; // soft-fail; caller treats as skipped
    throw err;
  }
}

/** Validate Cedar syntax + schema via the SW + wasm. When the bridge
 *  isn't available (extension missing), returns `{ ok: true, skipped: true }`
 *  so the UI shows a soft "검증 건너뜀" hint. The server rejects
 *  malformed text on save anyway. */
export async function validatePolicyLocal(
  cedarText: string,
): Promise<ValidateResp> {
  try {
    const raw = await sendToExtension<string>(
      { type: "cedar-validate", text: cedarText },
      BRIDGE_TIMEOUT_MS,
    );
    // SW returns the raw JSON string the wasm produced. Shape:
    //   { ok: true }  |  { ok: false, errors: [{ message: string, ... }] }
    const parsed = JSON.parse(raw) as
      | { ok: true }
      | { ok: false; errors?: Array<{ message?: string }>; error?: string };
    if (parsed.ok) return { ok: true };
    const msg =
      parsed.errors?.map((e) => e.message).filter(Boolean).join("; ") ||
      parsed.error ||
      "cedar validation failed";
    return { ok: false, error: msg };
  } catch (err) {
    if (isMissingBridge(err)) return { ok: true, skipped: true };
    return { ok: false, error: err instanceof Error ? err.message : String(err) };
  }
}

/** Test a single Cedar policy against a request. */
export async function testPolicyLocal(
  cedarText: string,
  request: CedarRequestInput,
): Promise<TestPolicyResp> {
  const raw = await sendToExtension<string>(
    {
      type: "cedar-test",
      text: cedarText,
      request_json: JSON.stringify(request),
    },
    BRIDGE_TIMEOUT_MS,
  );
  return JSON.parse(raw) as TestPolicyResp;
}

/** Simulate a sequence of requests against an entire policy set. */
export async function simulateSequenceLocal(
  steps: SequenceStepInput[],
  policies: PolicyInput[],
): Promise<SequenceResp> {
  const raw = await sendToExtension<string>(
    {
      type: "cedar-simulate",
      steps_json: JSON.stringify(steps),
      policies_json: JSON.stringify(policies),
    },
    BRIDGE_TIMEOUT_MS,
  );
  return JSON.parse(raw) as SequenceResp;
}

// ── block-IR bridge (text↔EST↔blocks) ────────────────────────────────────

/** Cedar text → block IR (one PolicyIR per policy in the text). Routes
 *  text→EST through the SW + wasm bridge, then converts EST→IR locally.
 *  `descriptor` (optional) drives schema annotations on `attr` nodes. */
export async function textToBlocks(
  cedarText: string,
  descriptor: SchemaDescriptor | null = null,
): Promise<PolicyIR[]> {
  const raw = await sendToExtension<string>(
    { type: "cedar-text-to-est", text: cedarText },
    BRIDGE_TIMEOUT_MS,
  );
  const parsed = JSON.parse(raw) as
    | { ok: true; policies: { id: string; est: unknown }[] }
    | { ok: false; error?: string };
  if (!parsed.ok) throw new Error(parsed.error ?? "cedar text→EST failed");
  return parsed.policies.map((p) =>
    estToBlocks(p.est as Parameters<typeof estToBlocks>[0], descriptor),
  );
}

/** Block IR → Cedar text. Converts IR→EST locally (throws on unfilled
 *  holes — call fillParams first), then routes EST→text through the bridge. */
export async function blocksToText(ir: PolicyIR): Promise<string> {
  const est = blocksToEst(ir);
  const raw = await sendToExtension<string>(
    { type: "cedar-est-to-text", est_json: JSON.stringify(est) },
    BRIDGE_TIMEOUT_MS,
  );
  const parsed = JSON.parse(raw) as
    | { ok: true; text: string }
    | { ok: false; error?: string };
  if (!parsed.ok) throw new Error(parsed.error ?? "cedar EST→text failed");
  return parsed.text;
}

/** Fetch the per-action typed field catalog from the SW (wasm-built). Returns a
 *  {@link SchemaDescriptor} keyed by the policy-facing action id (e.g. `"Swap"`)
 *  — pass it straight into {@link textToBlocks} for schema annotations. Fetch
 *  once and reuse across policies. Soft-fails to `null` when the extension
 *  bridge is unavailable, so the UI still renders blocks (without annotations). */
export async function fetchFieldCatalog(): Promise<SchemaDescriptor | null> {
  try {
    return await sendToExtension<SchemaDescriptor>(
      { type: "manifest:get-field-catalog" },
      BRIDGE_TIMEOUT_MS,
    );
  } catch (err) {
    if (isMissingBridge(err)) return null;
    throw err;
  }
}
