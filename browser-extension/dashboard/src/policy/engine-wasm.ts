import init, {
  evaluate_policy_rpc_json,
  install_policies_json,
  plan_policy_rpc_json,
} from "../wasm/policy_engine_wasm.js";
import type { Envelope, VerdictDto } from "./types";

let initPromise: Promise<unknown> | null = null;
async function ensureReady(): Promise<void> {
  if (!initPromise) initPromise = init();
  await initPromise;
}

// ── individual call wrappers ───────────────────────────────────────────────

export interface InstallInput {
  schemaText?: string;
  policySet: Array<{ id: string; text: string }>;
  manifests?: unknown[];
}

export interface InstallResult {
  ok: boolean;
  error?: { kind?: string; message?: string };
  data?: unknown;
}

export async function installPolicies(
  input: InstallInput,
): Promise<InstallResult> {
  await ensureReady();
  const payload = {
    schema_text: input.schemaText ?? "",
    policy_set: input.policySet,
    manifests: input.manifests ?? [],
  };
  const raw = install_policies_json(JSON.stringify(payload));
  const env = JSON.parse(raw) as Envelope<unknown>;
  return { ok: env.ok, error: env.error, data: env.data };
}

// PolicyRpcPlanDto opaque to the dashboard — we just pass it back to
// evaluate verbatim. The shape contains hashes the engine cross-checks.
export interface PlanDto {
  request_id: string;
  manifest_set_hash: string;
  schema_hash: string;
  [k: string]: unknown;
}

export interface RawRequestInput {
  method: string;
  params: unknown;
  chainId: number;
  blockTimestamp?: number;
}

export interface PlanResult {
  plan?: PlanDto;
  error?: { kind?: string; message?: string };
}

export async function planPolicyRpc(
  requestId: string,
  raw: RawRequestInput,
  manifests: unknown[] = [],
): Promise<PlanResult> {
  await ensureReady();
  const payload = {
    request_id: requestId,
    raw_request: {
      method: raw.method,
      params: raw.params,
      chain_id: raw.chainId,
      ...(raw.blockTimestamp ? { block_timestamp: raw.blockTimestamp } : {}),
    },
    manifests,
  };
  const r = plan_policy_rpc_json(JSON.stringify(payload));
  const env = JSON.parse(r) as Envelope<PlanDto>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { plan: env.data };
}

export interface EvaluateResult {
  verdict?: VerdictDto;
  error?: { kind?: string; message?: string };
}

export async function evaluatePolicyRpc(
  inputJson: string,
): Promise<EvaluateResult> {
  await ensureReady();
  const raw = evaluate_policy_rpc_json(inputJson);
  const env = JSON.parse(raw) as Envelope<VerdictDto>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { verdict: env.data };
}

// ── high-level orchestration ───────────────────────────────────────────────

export interface PolicyTestInput {
  policyId: string;
  cedarText: string;
  schemaText?: string;
  rawRequest: RawRequestInput;
}

export interface PolicyTestOutcome {
  verdict?: VerdictDto;
  /** The plan generated for this raw_request — surfaced so the UI can
   *  show which adapters/envelopes matched the call. */
  plan?: PlanDto;
  stage?: "install" | "plan" | "evaluate";
  error?: { kind?: string; message?: string };
}

// Full Policy Test pipeline against the dashboard's in-process WASM
// engine (separate from the extension SW's engine). Each stage stops on
// failure and reports which stage broke so the user can correct input.
export async function runPolicyTest(
  input: PolicyTestInput,
): Promise<PolicyTestOutcome> {
  await ensureReady();

  const installRes = await installPolicies({
    schemaText: input.schemaText ?? "",
    policySet: [{ id: input.policyId, text: input.cedarText }],
  });
  if (!installRes.ok) {
    return { stage: "install", error: installRes.error };
  }

  const requestId = `dashboard-${Date.now()}-${Math.random()
    .toString(36)
    .slice(2)}`;
  const planRes = await planPolicyRpc(requestId, input.rawRequest);
  if (!planRes.plan) {
    return { stage: "plan", error: planRes.error };
  }

  const evalInput = {
    plan: planRes.plan,
    rpc_response: {
      request_id: requestId,
      results: [] as unknown[],
    },
    manifests: [] as unknown[],
  };
  const evalRes = await evaluatePolicyRpc(JSON.stringify(evalInput));
  if (!evalRes.verdict) {
    return { stage: "evaluate", plan: planRes.plan, error: evalRes.error };
  }
  return { verdict: evalRes.verdict, plan: planRes.plan };
}
