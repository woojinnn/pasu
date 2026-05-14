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
  const response = await fetch(`${policyRpcUrl.replace(/\/+$/, "")}/v1/rpc`, {
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
  return body;
}

function defaultPolicyRpcUrl(): string {
  return process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";
}
