import { tryHandleLocally } from "./local-method-handlers";
import { fetchStarted, fetchEnded } from "./diagnostics";
// `./scopeball-auth/*` is imported LAZILY inside the authenticated dispatch path
// only — it pulls `webextension-polyfill` (browser-only), which would otherwise
// break every (test) importer of this module at load time.
import type {
  PolicyRpcBatchRequestDto,
  PlannedCallV2Dto,
  PolicyRpcCallDto,
  PolicyRpcResponseDto,
  VerdictDto,
} from "./wasm-bridge.types";

/**
 * Context the authenticated `/evaluate` enrichment path needs to build the
 * server's `EvaluateRequest`. The tx verdict paths already have all three.
 */
export interface ServerEvalContext {
  readonly action: unknown;
  readonly meta: unknown;
  readonly tx: { readonly chain_id: string; readonly from: string; readonly to: string };
}

/**
 * Serve remote enrichment from the authenticated policy-server `/evaluate`
 * (which executes e.g. `oracle.usd_value` from the signed-in user's synced
 * holding price) instead of the legacy per-method rpc server. Returns the
 * `{ call_id: result }` map from `policyRequest.results`. Throws on transport /
 * auth failure so the caller can fail CLOSED (omit → required `SystemFail` deny).
 */
/** True when a signed-in server session token is present (lazy polyfill load). */
async function hasServerSession(): Promise<boolean> {
  try {
    const { getAccessToken } = await import("./scopeball-auth/tokenStore");
    return (await getAccessToken()) != null;
  } catch {
    return false;
  }
}

async function serveEnrichmentViaEvaluate(
  remoteCallIds: ReadonlySet<string>,
  planned: readonly PlannedCallV2Dto[],
  ctx: ServerEvalContext,
): Promise<Record<string, unknown>> {
  const { evaluate: serverEvaluate } = await import("./scopeball-auth/client");
  // `PlannedCallV2Dto` IS the server `CallSpec` shape (manifest_id / call_id /
  // method / params / outputs / optional) — forward the remote subset verbatim.
  const callSpecs = planned.filter((c) => remoteCallIds.has(c.call_id));
  const response = await serverEvaluate({
    wallet_id: { address: ctx.tx.from, chains: [ctx.tx.chain_id] },
    envelopes: [{ meta: ctx.meta, body: ctx.action } as Record<string, unknown>],
    eval_context: {
      // Field names + enum variants must match the server's `EvalContext`
      // (asset-model/state/eval_context.rs): `request_kind` is camelCase,
      // `simulation` (NOT `simulation_mode`) is snake_case, and `action_index`
      // is a REQUIRED field (no serde default) — omitting any of these makes the
      // server reject the whole request with 422 → enrichment fail-closed.
      chain: ctx.tx.chain_id,
      now: Math.floor(Date.now() / 1000),
      action_index: 0,
      request_kind: "transaction",
      simulation: "preview",
    },
    call_specs: callSpecs as unknown as ReadonlyArray<Record<string, unknown>>,
  });
  const pr = response.policyRequest as { results?: unknown } | undefined;
  const out: Record<string, unknown> = {};
  if (pr && typeof pr.results === "object" && pr.results !== null) {
    for (const [k, v] of Object.entries(pr.results as Record<string, unknown>)) {
      out[k] = v;
    }
  }
  return out;
}

// ── Dormant v3 JSON-RPC 2.0 client ────────────────────────────────────────
//
// Kept for experiments with a typed action channel between the service worker
// and a stateful RPC server. The active extension verdict path is the v2
// ActionBody pipeline below: `plan_action_rpc_v2_json` → host dispatch →
// `evaluate_action_v2_json`.
//
// Wire shape (request body):
//   { jsonrpc: "2.0", method: "scopeball.evaluate_v3",
//     params: { wallet_id, actions, eval_context }, id: <unique> }
//
// Wire shape (success response):
//   { jsonrpc: "2.0", id, result: { policyRequest, diagnostics } }
//
// Wire shape (error response — JSON-RPC 2.0 §5.1):
//   { jsonrpc: "2.0", id, error: { code, message, data? } }
//
// There is no standalone `policy-rpc/` package in this worktree. Do not treat
// this client as the active transaction driver unless a future change wires it
// back into the orchestrator.

/**
 * JSON-RPC 2.0 (§5.1) error body. The rpc-server may include arbitrary
 * structured `data` next to the code/message pair — we keep it as
 * `unknown` so the SW can surface it in audit logs without binding the
 * shape ahead of time.
 */
export interface RpcErrorBody {
  readonly code: number;
  readonly message: string;
  readonly data?: unknown;
}

/**
 * Thrown when the rpc-server returns a JSON-RPC 2.0 error object
 * (`{ jsonrpc, id, error }`) instead of `{ jsonrpc, id, result }`. The
 * caller can inspect `code` (numeric, per spec) and `data` for
 * routing decisions; the rendered `message` carries the human text.
 */
export class RpcError extends Error {
  readonly code: number;
  readonly data?: unknown;

  constructor(code: number, message: string, data?: unknown) {
    super(`policy-rpc error ${code}: ${message}`);
    this.name = "RpcError";
    this.code = code;
    this.data = data;
  }
}

/**
 * Phase 5A — opaque diagnostic record the rpc-server may attach to a
 * `scopeball.evaluate_v3` reply. The SW renders these into the audit
 * log alongside the verdict so a faulty reducer / sync orchestrator
 * trip is surfaceable without re-routing through the verdict path.
 *
 * Fields are intentionally `unknown`-typed: the rpc-server contract is
 * still moving, and binding the shape now risks ABI drift between SW
 * and server. Add specific fields in Phase 6 once the shape stabilises.
 */
export interface Diagnostic {
  readonly kind?: string;
  readonly message?: string;
  readonly [key: string]: unknown;
}

/**
 * Phase 5A — opaque `PolicyRequest` payload the rpc-server returns.
 * The SW does not introspect this object today; it hands the JSON
 * verbatim to `evaluate_policy_request_json` for Cedar evaluation. The
 * field set will firm up in Phase 6 once the reducer body + state
 * sync are wired; until then anything beyond "JSON-serialisable
 * object" is up to the server.
 */
export interface PolicyRequest {
  readonly actions?: readonly unknown[];
  readonly state_before?: unknown;
  readonly deltas?: readonly unknown[];
  readonly state_after?: unknown;
  readonly [key: string]: unknown;
}

/**
 * Wallet identity the SW asserts to the rpc-server. EVM address + the
 * chain set the wallet currently tracks. Mirrors the `WalletId` type
 * the WASM-side simulation crate emits via tsify (see
 * `browser-extension/backend/wasm/policy_engine_wasm.d.ts`).
 *
 * NOTE: We do not import the wasm-generated `WalletId` directly here
 * because that .d.ts is a build artefact (regenerated by `wasm-pack`)
 * — re-stating the shape locally keeps `policy-rpc.ts` decoupled from
 * the build cycle. Phase 6 may switch to the wasm export once the
 * type is stable.
 */
export interface WalletId {
  readonly address: string;
  readonly chains: readonly string[];
}

/**
 * Phase 5A — opaque `Action` payload. Same rationale as `PolicyRequest`:
 * the schema is in flux while Phase 1 of the FSM plan is finalising the
 * Rust source-of-truth. The SW carries actions verbatim through to the
 * rpc-server, which alone needs to interpret them.
 */
export type Action = Record<string, unknown>;

/**
 * Phase 5A — opaque `EvalContext` payload. Mirrors
 * `policy_state::EvalContext` (chain + clock + RequestKind +
 * SimulationMode + envelope_index). Carried verbatim by the SW.
 */
export type EvalContext = Record<string, unknown>;

/** JSON-RPC 2.0 (§4) request envelope. */
interface JsonRpcRequest<P> {
  readonly jsonrpc: "2.0";
  readonly method: string;
  readonly params: P;
  readonly id: string;
}

/** JSON-RPC 2.0 (§5) reply envelope (success or error). */
interface JsonRpcReply<R> {
  readonly jsonrpc?: "2.0";
  readonly id?: string | number | null;
  readonly result?: R;
  readonly error?: RpcErrorBody;
}

/**
 * Resolve the v3 rpc-server base URL. Webpack's dotenv plugin injects
 * `process.env.POLICY_RPC_BASE_URL` at build time (see
 * `webpack/webpack.common.js`); we fall back to the legacy
 * `POLICY_RPC_URL` so a worktree that pre-dates the Phase 5 split keeps
 * working, then finally to the local-dev default.
 */
function defaultPolicyRpcBaseUrl(): string {
  if (typeof process === "undefined") return "http://127.0.0.1:8787";
  return (
    process.env.POLICY_RPC_BASE_URL ??
    process.env.POLICY_RPC_URL ??
    "http://127.0.0.1:8787"
  );
}

/**
 * Phase 5A — generate a per-request id for the JSON-RPC 2.0 envelope.
 * Strictly local: the rpc-server echoes the id back in the reply, so a
 * collision could let two concurrent evaluations cross-pollinate
 * verdicts. `crypto.randomUUID` is available in MV3 service workers;
 * we fall back to a monotonic counter just in case.
 */
let monotonicId = 0;
function generateRequestId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  monotonicId += 1;
  return `evaluate-v3:${Date.now()}:${monotonicId}`;
}

/**
 * Phase 5A — call the rpc-server's `scopeball.evaluate_v3` method.
 *
 * Returns the rpc-server's `policyRequest` payload (an opaque object the
 * SW hands to `evaluate_policy_request_json` for Cedar evaluation) plus
 * any `diagnostics` records the server attached.
 *
 * Throws:
 *   * `RpcError(code, message, data?)` — rpc-server returned a
 *     JSON-RPC 2.0 error response. Caller can inspect `code` for routing.
 *   * Generic `Error` — transport failure (HTTP non-2xx, network reset,
 *     malformed JSON, missing `result` field on a 200). If this dormant path
 *     is reactivated, transport faults must be fenced so wallet decisions
 *     still fail closed.
 */
export async function evaluateV3(
  actions: readonly Action[],
  evalContext: EvalContext,
  walletId: WalletId,
  options: { policyRpcBaseUrl?: string } = {},
): Promise<{ policyRequest: PolicyRequest; diagnostics: readonly Diagnostic[] }> {
  const baseUrl = options.policyRpcBaseUrl ?? defaultPolicyRpcBaseUrl();
  const id = generateRequestId();
  const body: JsonRpcRequest<{
    readonly wallet_id: WalletId;
    readonly actions: readonly Action[];
    readonly eval_context: EvalContext;
  }> = {
    jsonrpc: "2.0",
    method: "scopeball.evaluate_v3",
    params: {
      wallet_id: walletId,
      actions,
      eval_context: evalContext,
    },
    id,
  };

  const url = `${baseUrl.replace(/\/+$/, "")}/`;
  const startedAtMs = Date.now();
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch (err) {
    // Network / DNS / abort — surface as transport failure. Any future caller
    // must map this to fail-closed behavior.
    throw new Error(
      `policy-rpc v3 transport failed: ${
        err instanceof Error ? err.message : String(err)
      }`,
    );
  }

  if (!response.ok) {
    throw new Error(
      `policy-rpc v3 returned HTTP ${response.status} ${response.statusText}`,
    );
  }

  let reply: JsonRpcReply<{
    policyRequest: PolicyRequest;
    diagnostics?: readonly Diagnostic[];
  }>;
  try {
    reply = (await response.json()) as JsonRpcReply<{
      policyRequest: PolicyRequest;
      diagnostics?: readonly Diagnostic[];
    }>;
  } catch (err) {
    throw new Error(
      `policy-rpc v3 returned malformed JSON: ${
        err instanceof Error ? err.message : String(err)
      }`,
    );
  }

  if (reply.error !== undefined) {
    throw new RpcError(reply.error.code, reply.error.message, reply.error.data);
  }
  if (reply.result === undefined || reply.result === null) {
    throw new Error("policy-rpc v3 reply is missing `result` field");
  }
  // We deliberately do NOT enforce `reply.id === id` — a permissive
  // server might rewrite the id (e.g. a transparent proxy). The
  // observability layer still logs both ids if drift becomes a debug
  // signal.
  const policyRequest = reply.result.policyRequest;
  if (policyRequest === undefined || policyRequest === null) {
    throw new Error("policy-rpc v3 reply.result is missing `policyRequest`");
  }
  const diagnostics = reply.result.diagnostics ?? [];
  console.debug("[Scopeball] policy-rpc.v3", {
    requestId: id,
    url,
    actionCount: actions.length,
    durationMs: Date.now() - startedAtMs,
    diagnosticsCount: diagnostics.length,
  });

  return { policyRequest, diagnostics };
}

export interface PolicyRpcAuditMeta {
  request_id: string;
  manifest_set_hash: string;
  schema_hash: string;
  call_ids: string[];
  methods: string[];
}

/**
 * Phase 1 / P2 — v2 (ActionBody-model) policy-RPC dispatch.
 *
 * Synthesises a [`PolicyRpcCallDto`] per planned call (`id = call.call_id`),
 * answers pure-math methods in-process via {@link tryHandleLocally}, POSTs the
 * remainder through the {@link postPolicyRpc} round-trip (127.0.0.1:8787
 * `/v1/rpc`), and folds the results into a `{ call_id: <value> }` map.
 *
 * Fold rule (fail-CLOSED):
 *   - `ok: true`  → `map[call_id] = result` (the UNWRAPPED `$.result`
 *     payload — exactly what `evaluate_action_v2_json.results` expects).
 *   - `ok: false`, unreachable daemon, missing/dropped call → OMITTED.
 *
 * We do NOT synthesise `rpc_unreachable` error stubs. v2 fails CLOSED: an
 * omitted REQUIRED (`optional === false`) call makes `materialize_v2` raise
 * `SystemFail`, which `evaluate_action_v2_json` turns into a `__system__`
 * fail-closed verdict. A down daemon therefore denies a required-RPC swap
 * rather than waving it through — the conservative posture for the v2 cutover.
 *
 * Returns an empty map when `planned` is empty (the no-call baseline).
 */
export async function dispatchCallsV2(
  planned: readonly PlannedCallV2Dto[],
  policyRpcUrl: string,
  ctx?: ServerEvalContext,
): Promise<Record<string, unknown>> {
  const results: Record<string, unknown> = {};
  if (planned.length === 0) return results;

  // Map every planned v2 call onto the policy-rpc batch DTO keyed by `call_id`, so
  // both the local handlers and the remote server correlate by the same id
  // the WASM materializer reads `results` under.
  const calls: PolicyRpcCallDto[] = planned.map((call) => ({
    id: call.call_id,
    method: call.method,
    params: call.params,
  }));

  const remoteCalls: PolicyRpcCallDto[] = [];
  for (const call of calls) {
    const local = tryHandleLocally(call);
    if (local) {
      // Keep only successful local results; drop failures (no error stub).
      if (local.ok) results[local.id] = local.result;
    } else {
      remoteCalls.push(call);
    }
  }

  if (remoteCalls.length === 0) {
    console.debug("[Scopeball] policy-rpc-v2 (all local)", {
      plannedCount: planned.length,
      resolvedCount: Object.keys(results).length,
    });
    return results;
  }

  // Preferred path: when the caller supplied a server-eval context AND the user
  // is signed in, serve remote enrichment from the authenticated policy-server
  // `/evaluate` (executes oracle.usd_value etc. from the user's synced state).
  // Failures fail CLOSED — omit, so a required call trips `SystemFail` → deny.
  if (ctx && (await hasServerSession())) {
    try {
      const served = await serveEnrichmentViaEvaluate(
        new Set(remoteCalls.map((c) => c.id)),
        planned,
        ctx,
      );
      Object.assign(results, served);
    } catch (err) {
      console.warn(
        "[Scopeball] /evaluate enrichment failed, omitting remote calls (fail-closed)",
        { callCount: remoteCalls.length, err },
      );
    }
    return results;
  }

  // Legacy path (signed-out / no context): the per-method rpc server only
  // accepts the action-model batch shape `{ request_id, calls }`.
  const requestId = `action-v2:${remoteCalls[0]?.id ?? "calls"}`;
  const remotePlan: PolicyRpcBatchRequestDto = {
    request_id: requestId,
    calls: remoteCalls,
  };

  let remoteResponse: PolicyRpcResponseDto;
  try {
    remoteResponse = await postPolicyRpc(policyRpcUrl, remotePlan);
  } catch (err) {
    // Fail CLOSED: do NOT synthesise error stubs. Omitting the calls means a
    // required one trips `SystemFail` → `__system__` deny inside WASM.
    console.warn(
      "[Scopeball] policy-rpc-v2 unreachable, omitting remote calls (fail-closed)",
      { requestId, callCount: remoteCalls.length, err },
    );
    return results;
  }

  // Fold the batch results: `ok: true` → unwrapped `result`; everything
  // else is omitted so a downstream required call fails closed.
  for (const entry of remoteResponse.results) {
    if (
      typeof entry !== "object" ||
      entry === null ||
      typeof (entry as { id?: unknown }).id !== "string"
    ) {
      continue;
    }
    const rec = entry as { id: string; ok?: unknown; result?: unknown };
    if (rec.ok === true) {
      results[rec.id] = rec.result;
    }
  }
  return results;
}

async function postPolicyRpc(
  policyRpcUrl: string,
  plan: PolicyRpcBatchRequestDto,
): Promise<PolicyRpcResponseDto> {
  const url = `${policyRpcUrl.replace(/\/+$/, "")}/v1/rpc`;
  const startedAtMs = Date.now();
  const traceSeq = fetchStarted("dispatch", url);
  console.info("[Scopeball] registry-fetch → sent", {
    label: "dispatch",
    url,
    sentAt: new Date(startedAtMs).toISOString(),
  });
  try {
    const response = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(plan),
    });
    fetchEnded(traceSeq, response.status, Date.now() - startedAtMs);
    console.info("[Scopeball] registry-fetch ← recv", {
      label: "dispatch",
      url,
      sentAt: new Date(startedAtMs).toISOString(),
      receivedAt: new Date().toISOString(),
      durationMs: Date.now() - startedAtMs,
      status: response.status,
    });
    if (!response.ok) {
      throw new Error(`policy-rpc returned HTTP ${response.status}`);
    }
    const body = (await response.json()) as PolicyRpcResponseDto;
    if (body.request_id !== plan.request_id || !Array.isArray(body.results)) {
      throw new Error("policy-rpc returned malformed response");
    }
    console.debug("[Scopeball] policy-rpc", {
      requestId: plan.request_id,
      url,
      callCount: plan.calls.length,
      status: response.status,
      durationMs: Date.now() - startedAtMs,
      resultCount: body.results.length,
      results: body.results,
    });
    return body;
  } catch (err) {
    fetchEnded(
      traceSeq,
      `error:${err instanceof Error ? err.message : String(err)}`,
      Date.now() - startedAtMs,
    );
    console.error("[Scopeball] policy-rpc failed", {
      requestId: plan.request_id,
      url,
      callCount: plan.calls.length,
      durationMs: Date.now() - startedAtMs,
      err,
    });
    throw err;
  }
}

/**
 * Sentinel `policy_id` WASM returns when a required RPC enrichment
 * call fails. The matched entry is synthesised by the engine, not
 * authored by any user policy, and the audit log must surface it as a
 * first-class verdict (not as a generic engine error).
 */
export const SYSTEM_POLICY_ID = "__system__";

/**
 * Audit-log "matched policy" shape. The base case is the existing
 * `{ id, severity }`; D9 system failures additionally carry the
 * runtime `reason` (e.g. `"rpc-unavailable: <call-id>"`) so the
 * dashboard can render a meaningful message.
 */
export interface AuditMatchedPolicy {
  id: string;
  severity: string;
  reason?: string;
}

/**
 * Project a `VerdictDto` into the matched-policies list that gets
 * persisted in the audit log. Preserves engine-emitted synthetic ids
 * (`__system__`, `__engine::*`) verbatim and propagates their `reason`
 * so the dashboard's audit view can distinguish "Cedar policy blocked
 * this" from "the engine couldn't evaluate it".
 */
export function formatAuditMatched(verdict: VerdictDto): AuditMatchedPolicy[] {
  if (!verdict.matched) return [];
  return verdict.matched.map((m) => {
    const base: AuditMatchedPolicy = { id: m.policy_id, severity: m.severity };
    // Synthetic engine matches carry a runtime diagnostic the dashboard
    // can't reconstruct from the catalog — keep the `reason`. Ordinary
    // policy matches stay payload-light; the dashboard pulls their
    // human text from the catalog by id.
    const isSynthetic =
      m.policy_id === SYSTEM_POLICY_ID || m.policy_id.startsWith("__engine::");
    if (isSynthetic && typeof m.reason === "string") {
      base.reason = m.reason;
    }
    return base;
  });
}
