/**
 * Cedar editor support — `/policies/validate` and `/policies/:id/test`.
 *
 * The editor calls validate on every keystroke (debounced) to show
 * inline syntax errors; the "test against TX" panel posts a fully-
 * formed Cedar request to /policies/:id/test for a live verdict.
 */

import type { PolicySeverity, Verdict } from "@scopeball/types";

import { request } from "./client";

export interface ValidateResp {
  ok: boolean;
  error?: string;
}

/** `POST /policies/validate` — parse-check only (no schema validation). */
export async function validatePolicy(cedar_text: string): Promise<ValidateResp> {
  return request<ValidateResp>("/policies/validate", {
    method: "POST",
    body: { cedar_text },
  });
}

/** Cedar request input — principal/action/resource as `Type::"id"` strings. */
export interface CedarRequestInput {
  principal: string;
  action: string;
  resource: string;
  /** Cedar entities array (JSON form Cedar accepts). */
  entities?: unknown[];
  /** Cedar context record. */
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

/** `POST /policies/:id/test` — evaluate saved policy schema-less against the supplied request. */
export async function testPolicy(
  id: number,
  request_: CedarRequestInput,
): Promise<TestPolicyResp> {
  return request<TestPolicyResp>(`/policies/${id}/test`, {
    method: "POST",
    body: { request: request_ },
  });
}

// Re-export for callers that want the severity tag explicitly.
export type { PolicySeverity };
