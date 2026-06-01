/**
 * Phase 5 — `/tx/decode`, `/approvals/revoke-plan`, `/simulate/sequence`.
 *
 * These power the Simulation page and the revoke-flow CTAs in
 * Monitoring. The shapes match `crates/simulation/server/src/phase5_handlers.rs`
 * verbatim — keep them in sync when fields are added.
 */

import type { Address, ChainId, PolicySeverity, Verdict } from "@scopeball/types";

import { request } from "./client";

// ── /tx/decode ─────────────────────────────────────────────────────────

export interface DecodeReq {
  chain?: ChainId;
  to: Address;
  data: string;
  value?: string;
}

export interface ActionHint {
  domain: string;
  kind: string;
}

export interface DecodeResp {
  chain: ChainId | null;
  to: Address;
  selector: string;
  function_signature: string | null;
  function_name: string | null;
  action_envelope: ActionHint | null;
  display_label: string;
}

/** `POST /tx/decode` — selector + action hint from raw calldata. */
export async function decodeTx(body: DecodeReq): Promise<DecodeResp> {
  return request<DecodeResp>("/tx/decode", { method: "POST", body });
}

// ── /approvals/revoke-plan ────────────────────────────────────────────

export interface RevokeItem {
  chain: ChainId;
  token: Address;
  spender: Address;
  label?: string;
}

export interface RevokeCall {
  chain: ChainId;
  to: Address;
  data: string;
  value: string;
  selector: string;
  label: string | null;
}

export interface RevokePlanResp {
  calls: RevokeCall[];
}

/** `POST /approvals/revoke-plan` — build approve(spender, 0) calldata per item. */
export async function planRevokes(items: RevokeItem[]): Promise<RevokePlanResp> {
  return request<RevokePlanResp>("/approvals/revoke-plan", {
    method: "POST",
    body: { items },
  });
}

// ── /simulate/sequence ────────────────────────────────────────────────

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

/** `POST /simulate/sequence` — batch-evaluate N Cedar requests across active policies. */
export async function simulateSequence(
  steps: SequenceStepInput[],
  policyIds?: number[],
): Promise<SequenceResp> {
  return request<SequenceResp>("/simulate/sequence", {
    method: "POST",
    body: { steps, policy_ids: policyIds },
  });
}
