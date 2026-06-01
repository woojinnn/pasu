/**
 * Thin TS wrapper around `@scopeball/cedar-wasm`.
 *
 * Background: simulation-server used to host `POST /policies/validate` +
 * `POST /policies/:id/test` + `POST /simulate/sequence`. The DB-only
 * server vision called for moving Cedar out of the server entirely;
 * `cedar-wasm-lite` (`crates/cedar-wasm-lite`) compiles `cedar-policy`
 * to a wasm module, and this module gives us a tiny TS-friendly
 * facade so pages don't have to manage init + JSON serialization.
 *
 * The wasm bundle is ~2.5 MB pre-gzip (~800 KB gzipped). Init is
 * lazy + memoized — the first call loads it; subsequent calls are
 * synchronous-ish (a microtask).
 */
import initWasm, {
  simulate_sequence,
  test_policy,
  validate_policy,
} from "@scopeball/cedar-wasm";

import type { PolicySeverity, Verdict } from "@scopeball/types";

// ── Init memoization ────────────────────────────────────────────────────

let initPromise: Promise<void> | null = null;

/** Idempotent wasm init. Safe to call from every page; only the first
 *  call actually fetches + instantiates. */
export function ensureCedarReady(): Promise<void> {
  if (!initPromise) {
    initPromise = initWasm().then(() => {
      // initWasm resolves with a WebAssembly.Instance handle; we don't
      // need it — the wasm-bindgen JS module keeps the global ref.
    });
  }
  return initPromise;
}

// ── public types (match the old api-client shapes) ──────────────────────

export interface ValidateResp {
  ok: boolean;
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

// ── wrappers ────────────────────────────────────────────────────────────

/** Replacement for the old `validatePolicy()` api-client wrapper. */
export async function validatePolicyLocal(cedarText: string): Promise<ValidateResp> {
  await ensureCedarReady();
  return JSON.parse(validate_policy(cedarText)) as ValidateResp;
}

/** Replacement for the old `testPolicy(id, req)` api-client wrapper.
 *  The id is not used here — wasm operates on raw cedar_text. Callers
 *  that previously passed (id, req) now pass (cedarText, req). */
export async function testPolicyLocal(
  cedarText: string,
  request_: CedarRequestInput,
): Promise<TestPolicyResp> {
  await ensureCedarReady();
  const raw = test_policy(cedarText, JSON.stringify(request_));
  const out = JSON.parse(raw) as TestPolicyResp & { error?: string };
  if (out.error) {
    // surface the wasm-side error as a fail verdict with synthetic matched
    return {
      verdict: "fail" as Verdict,
      matched: [{ policy_id: "__error__", severity: "deny", reason: out.error }],
    };
  }
  return { verdict: out.verdict, matched: out.matched };
}

/** Replacement for the old `simulateSequence(steps, policyIds)`. We
 *  don't filter on id here — callers pre-select which policies to
 *  feed in. Empty list of policies → every step is `pass`. */
export async function simulateSequenceLocal(
  steps: SequenceStepInput[],
  policies: PolicyInput[],
): Promise<SequenceResp> {
  await ensureCedarReady();
  const raw = simulate_sequence(JSON.stringify(steps), JSON.stringify(policies));
  return JSON.parse(raw) as SequenceResp;
}
