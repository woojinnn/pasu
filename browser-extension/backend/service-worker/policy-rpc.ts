import { tryHandleLocally } from "./local-method-handlers";
import { evaluatePolicyRpc, planPolicyRpc } from "./wasm-bridge";
import type {
  PolicyRpcCallDto,
  PolicyRpcPlanDto,
  PolicyRpcResponseDto,
  VerdictDto,
} from "./wasm-bridge.types";
import {
  isTransaction,
  isTypedSignature,
  type Message,
} from "@lib/types";

export interface PolicyRpcAuditMeta {
  request_id: string;
  manifest_set_hash: string;
  schema_hash: string;
  call_ids: string[];
  methods: string[];
}

interface EvaluateWithPolicyRpcOptions {
  policyRpcUrl?: string;
  manifests?: readonly unknown[];
}

export async function evaluateWithPolicyRpc(
  message: Message,
  options: EvaluateWithPolicyRpcOptions = {},
): Promise<{ verdict: VerdictDto; audit: PolicyRpcAuditMeta }> {
  const manifests = [...(options.manifests ?? [])];
  const plan = await planPolicyRpc({
    request_id: message.requestId,
    raw_request: rawRequestFromMessage(message),
    manifests,
  });
  const rpcResponse = await dispatchCalls(
    plan,
    options.policyRpcUrl ?? defaultPolicyRpcUrl(),
  );
  const verdict = await evaluatePolicyRpc({
    plan,
    rpc_response: rpcResponse,
    manifests,
  });

  return {
    verdict,
    audit: {
      request_id: plan.request_id,
      manifest_set_hash: plan.manifest_set_hash,
      schema_hash: plan.schema_hash,
      call_ids: plan.calls.map((call) => call.id),
      methods: plan.calls.map((call) => call.method),
    },
  };
}

function rawRequestFromMessage(message: Message): {
  method: string;
  params: unknown;
  chain_id: number;
  block_timestamp: number;
} {
  const block_timestamp = Math.floor(Date.now() / 1000);
  if (isTransaction(message)) {
    return {
      method: "eth_sendTransaction",
      params: [message.data.transaction],
      chain_id: message.data.chainId,
      block_timestamp,
    };
  }
  if (isTypedSignature(message)) {
    return {
      method: "eth_signTypedData_v4",
      params: [message.data.address, message.data.typedData],
      chain_id: message.data.chainId,
      block_timestamp,
    };
  }
  throw new Error("unsupported message type for policy-rpc");
}

/**
 * Split the plan's calls into ones we can answer in-process (pure-math
 * derivations from the action envelope, e.g. `token.normalize_to_nano`)
 * and ones that need the remote enrichment server (oracles, chain RPC,
 * portfolio lookup). Local handlers run before the HTTP round-trip so a
 * fully-local plan never touches the network — keeping policies usable
 * even when the policy-rpc daemon isn't reachable.
 *
 * Order is preserved by call id: the WASM materializer correlates results
 * to plan entries via `id`, so concatenating the two batches is safe
 * regardless of which calls were diverted.
 */
async function dispatchCalls(
  plan: PolicyRpcPlanDto,
  policyRpcUrl: string,
): Promise<PolicyRpcResponseDto> {
  if (plan.calls.length === 0) {
    return { request_id: plan.request_id, results: [] };
  }

  const localResults: unknown[] = [];
  const remoteCalls: PolicyRpcCallDto[] = [];
  for (const call of plan.calls) {
    const local = tryHandleLocally(call);
    if (local) {
      localResults.push(local);
    } else {
      remoteCalls.push(call);
    }
  }

  if (remoteCalls.length === 0) {
    console.debug("[Scopeball] policy-rpc (all local)", {
      requestId: plan.request_id,
      localCount: localResults.length,
    });
    return { request_id: plan.request_id, results: localResults };
  }

  const remotePlan: PolicyRpcPlanDto = { ...plan, calls: remoteCalls };
  // Fail-open on transport failure: when the policy-rpc daemon is down,
  // synthesise empty error results for each remote call instead of
  // throwing. Manifests mark their calls `optional: true`, so the engine
  // already handles missing enrichment by leaving the corresponding
  // context fields undefined and letting `context has X` guards short-
  // circuit. Throwing here would convert "no enrichment server" into
  // `__engine::unexpected` deny on every transaction.
  let remoteResponse: PolicyRpcResponseDto;
  try {
    remoteResponse = await postPolicyRpc(policyRpcUrl, remotePlan);
  } catch (err) {
    console.warn(
      "[Scopeball] policy-rpc unreachable, treating remote calls as missing",
      { requestId: plan.request_id, callCount: remoteCalls.length, err },
    );
    remoteResponse = {
      request_id: plan.request_id,
      results: remoteCalls.map((call) => ({
        id: call.id,
        ok: false,
        error: { code: "rpc_unreachable", message: String(err) },
      })),
    };
  }
  return {
    request_id: plan.request_id,
    results: [...localResults, ...remoteResponse.results],
  };
}

async function postPolicyRpc(
  policyRpcUrl: string,
  plan: PolicyRpcPlanDto,
): Promise<PolicyRpcResponseDto> {
  const url = `${policyRpcUrl.replace(/\/+$/, "")}/v1/rpc`;
  const startedAtMs = Date.now();
  try {
    const response = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ request_id: plan.request_id, calls: plan.calls }),
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

function defaultPolicyRpcUrl(): string {
  return process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";
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
