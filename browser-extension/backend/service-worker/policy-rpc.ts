import { evaluatePolicyRpc, planPolicyRpc } from "./wasm-bridge";
import type {
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
  const rpcResponse =
    plan.calls.length === 0
      ? { request_id: plan.request_id, results: [] }
      : await postPolicyRpc(options.policyRpcUrl ?? defaultPolicyRpcUrl(), plan);
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
 * persisted in the audit log. Preserves the `__system__` id verbatim
 * (D9) and propagates its `reason` so the dashboard's audit view can
 * distinguish "Cedar policy blocked this" from "the engine couldn't
 * evaluate it".
 */
export function formatAuditMatched(verdict: VerdictDto): AuditMatchedPolicy[] {
  if (!verdict.matched) return [];
  return verdict.matched.map((m) => {
    const base: AuditMatchedPolicy = { id: m.policy_id, severity: m.severity };
    // Only attach `reason` for the synthetic `__system__` matched entry
    // — ordinary policy matches keep the payload small (the dashboard
    // already has the reason in the catalog).
    if (m.policy_id === SYSTEM_POLICY_ID && typeof m.reason === "string") {
      base.reason = m.reason;
    }
    return base;
  });
}
