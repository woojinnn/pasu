import { tryHandleLocally } from "./local-method-handlers";
import { fetchStarted, fetchEnded } from "./diagnostics";
// `./pasu-auth/*` is imported LAZILY inside the authenticated dispatch path
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
    const { getAccessToken } = await import("./pasu-auth/tokenStore");
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
  const { evaluate: serverEvaluate } = await import("./pasu-auth/client");
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
// Kept for experimentation with a stateful RPC server channel. Not wired into
// the active verdict path (`plan_action_rpc_v2_json` → host dispatch →
// `evaluate_action_v2_json`).
//
// Wire shape (request):  { jsonrpc:"2.0", method:"pasu.evaluate_v3",
//                          params:{ wallet_id, actions, eval_context }, id }
// Wire shape (success):  { jsonrpc:"2.0", id, result:{ policyRequest, diagnostics } }
// Wire shape (error §5.1): { jsonrpc:"2.0", id, error:{ code, message, data? } }

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
 * Opaque diagnostic record the rpc-server may attach to a `pasu.evaluate_v3`
 * reply. Fields are `unknown`-typed to avoid ABI drift.
 */
export interface Diagnostic {
  readonly kind?: string;
  readonly message?: string;
  readonly [key: string]: unknown;
}

/**
 * Opaque `PolicyRequest` payload returned by the rpc-server.
 * The SW carries this verbatim; the shape is intentionally loose.
 */
export interface PolicyRequest {
  readonly actions?: readonly unknown[];
  readonly state_before?: unknown;
  readonly deltas?: readonly unknown[];
  readonly state_after?: unknown;
  readonly [key: string]: unknown;
}

/**
 * Wallet identity the SW asserts to the rpc-server. Stated locally to keep
 * this module decoupled from the wasm-pack build artefact.
 */
export interface WalletId {
  readonly address: string;
  readonly chains: readonly string[];
}

/** Opaque `Action` payload. Carried verbatim to the rpc-server. */
export type Action = Record<string, unknown>;

/** Opaque `EvalContext` payload. Carried verbatim by the SW. */
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
 * Resolve the v3 rpc-server base URL from build-time env vars,
 * falling back to the local-dev default.
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
 * Generate a per-request id for the JSON-RPC 2.0 envelope. Collision would
 * let two concurrent evaluations cross-pollinate verdicts.
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
 * Call the rpc-server's `pasu.evaluate_v3` method (dormant path).
 *
 * Returns the `policyRequest` payload plus any `diagnostics`.
 * Throws `RpcError` on a server error response, `Error` on transport failure.
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
    method: "pasu.evaluate_v3",
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
  // Do not enforce `reply.id === id` — a transparent proxy may rewrite it.
  const policyRequest = reply.result.policyRequest;
  if (policyRequest === undefined || policyRequest === null) {
    throw new Error("policy-rpc v3 reply.result is missing `policyRequest`");
  }
  const diagnostics = reply.result.diagnostics ?? [];
  console.debug("[Pasu] policy-rpc.v3", {
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
 * v2 (ActionBody-model) policy-RPC dispatch.
 *
 * For each planned call: handles pure-math methods in-process via
 * {@link tryHandleLocally}, POSTs the remainder to the policy-rpc server, and
 * folds the results into a `{ call_id: <value> }` map.
 *
 * Fail-closed: `ok: false` / unreachable / dropped calls are OMITTED. An
 * omitted REQUIRED call causes `materialize_v2` to raise `SystemFail`, which
 * `evaluate_action_v2_json` turns into a `__system__` deny — a down server
 * blocks a required-RPC swap rather than waiving it through.
 */
export async function dispatchCallsV2(
  planned: readonly PlannedCallV2Dto[],
  policyRpcUrl: string,
  ctx?: ServerEvalContext,
): Promise<Record<string, unknown>> {
  const results: Record<string, unknown> = {};
  if (planned.length === 0) return results;

  // Build the policy-rpc batch DTO keyed by `call_id` so local handlers and the
  // remote server both correlate by the id the WASM materializer reads `results` under.
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
    console.debug("[Pasu] policy-rpc-v2 (all local)", {
      plannedCount: planned.length,
      resolvedCount: Object.keys(results).length,
    });
    return results;
  }

  // When the user is signed in, serve remote enrichment from the authenticated
  // policy-server `/evaluate` (executes oracle.usd_value from the synced state).
  // Failures fail closed — omit, so a required call trips `SystemFail` → deny.
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
        "[Pasu] /evaluate enrichment failed, omitting remote calls (fail-closed)",
        { callCount: remoteCalls.length, err },
      );
    }
    return results;
  }

  // Fallback (signed-out / no context): post to the per-method rpc server.
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
      "[Pasu] policy-rpc-v2 unreachable, omitting remote calls (fail-closed)",
      { requestId, callCount: remoteCalls.length, err },
    );
    return results;
  }

  // Fold: `ok: true` → unwrapped `result`; anything else is omitted (fail closed).
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
  console.info("[Pasu] registry-fetch → sent", {
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
    console.info("[Pasu] registry-fetch ← recv", {
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
    console.debug("[Pasu] policy-rpc", {
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
    console.error("[Pasu] policy-rpc failed", {
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
 * Audit-log "matched policy" shape. Engine-emitted synthetic entries
 * additionally carry a `reason` so the dashboard can render a meaningful message.
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
    // Engine-emitted synthetic matches carry a runtime diagnostic; keep the `reason`.
    // Ordinary policy matches are payload-light; the dashboard pulls their text by id.
    const isSynthetic =
      m.policy_id === SYSTEM_POLICY_ID || m.policy_id.startsWith("__engine::");
    if (isSynthetic && typeof m.reason === "string") {
      base.reason = m.reason;
    }
    return base;
  });
}
